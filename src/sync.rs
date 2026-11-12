//! Catalog sync. An App Index or AltStore catalog is one JSON document: it is downloaded, its
//! format recognized, and imported when its change stamp moved ([`run_document`]).
//!
//! An F-Droid repository fetches the small `entry.json` first: if the timestamp is unchanged (and
//! so is the locale) the repo is up to date for one tiny request. Otherwise it obtains the new index —
//! preferring an **incremental** JSON Merge Patch diff over a full re-download when `entry.json`
//! offers one for our stored timestamp — caches the raw index, and applies it to the catalog with
//! [`catalog::upsert_index`]. Any problem with the diff falls back to a full download.
//!
//! Each sync runs on its own thread and blocks on the catalog worker's replies, so the import,
//! like the download, stays off the UI thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use day::prelude::*;

use crate::catalog::{self, block_on, format};
use crate::fdroid::{Entry, FileRef, IndexV2};
use crate::net::{self, Progress};
use crate::state::{self, SyncUi};
use crate::{cache, settings};

/// The locale used to pick localized index fields — the device's, so catalog descriptions and
/// screenshots match the device language. `fdroid::pick` falls back (e.g. `fr-FR` → `fr` → `en-US`
/// → `en`) so partial translations degrade gracefully. F-Droid's primary is `en-US`.
pub fn locale() -> String {
    crate::platform::device_locale()
}

/// Check every enabled repository for updates, when the launch preference allows it. The
/// catalog always holds the default repository, so a first launch syncs it.
pub fn sync_at_launch() {
    if settings::SYNC_ON_LAUNCH.get_untracked() {
        sync_all_enabled();
    }
}

/// Check every enabled repository for updates.
pub fn sync_all_enabled() {
    let repos = catalog::read(catalog::repos);
    day::task(async move {
        match repos.await {
            Ok(repos) => {
                for repo in repos.into_iter().filter(|r| r.enabled) {
                    start(repo.address);
                }
            }
            Err(e) => error!("sync: {e}"),
        }
    });
}

/// Load the image loader's mirror map from the stored catalog.
pub fn refresh_mirrors() {
    let map = catalog::read(catalog::mirror_map);
    day::task(async move {
        match map.await {
            Ok(map) => crate::icons::set_mirror_bases(map),
            Err(e) => warn!("mirrors: {e}"),
        }
    });
}

/// A background update check (#7): sync every enabled repo, then count how many of `installed`
/// (package, on-device versionCode) have a newer version in the catalog. Synchronous — the
/// WorkManager worker calls it on its own thread and posts a notification with the count. Sync
/// errors on any one repo are ignored so a single unreachable mirror doesn't sink the whole check.
// Called only from the Android `UpdateWorker` JNI export; dead on desktop/mock builds.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn check_updates_now(installed: Vec<(String, i64)>) -> i64 {
    let repos = block_on(catalog::read(catalog::repos)).unwrap_or_default();
    for repo in repos.iter().filter(|r| r.enabled) {
        let _ = run_any(&repo.address);
    }
    block_on(catalog::read(move |db| {
        let mut n = 0;
        for (pkg, code) in installed {
            if catalog::summary_by_pkg(db, &pkg)?.is_some_and(|s| s.version_code > code) {
                n += 1;
            }
        }
        Ok(n)
    }))
    .unwrap_or_default()
}

/// Sync one catalog in the background. `address` is an F-Droid repository's base (no index
/// filename) or an App Index / AltStore document's URL (`model::catalog_address`).
pub fn start(address: String) {
    thread::spawn(move || {
        crate::util::lower_priority();
        if let Err(e) = run_any(&address) {
            state::set_sync(SyncUi::Error(e));
        }
    });
}

/// Sync the catalog at `address` by its format: the one recorded at its last sync, else an
/// F-Droid repository for a base address and a document for a `.json` URL.
fn run_any(address: &str) -> Result<(), String> {
    let owned = address.to_string();
    let format = block_on(catalog::write(move |db| {
        let id = catalog::ensure_repo(db, &owned)?;
        Ok(catalog::repo(db, id)?.map(|r| r.format).unwrap_or_default())
    }))
    .map_err(|e| format!("db: {e}"))?;
    let fdroid = match format.as_str() {
        format::FDROID => true,
        "" => !address.ends_with(".json"),
        _ => false,
    };
    if fdroid {
        run(address, "index-v2.json")
    } else {
        run_document(address)
    }
}

fn run(address: &str, index_file: &str) -> Result<(), String> {
    state::set_sync(SyncUi::Checking);

    let owned = address.to_string();
    let repo = block_on(catalog::write(move |db| {
        let id = catalog::ensure_repo(db, &owned)?;
        catalog::repo(db, id)
    }))
    .map_err(|e| format!("db: {e}"))?
    .ok_or_else(|| "db: the catalog was removed".to_string())?;
    if repo.format.is_empty() {
        let id = repo.id;
        block_on(catalog::write(move |db| {
            catalog::set_repo_format(db, id, format::FDROID)
        }))
        .map_err(|e| format!("db: {e}"))?;
    }
    let stored = repo.timestamp;
    let locale = locale();
    // A locale change re-picks every localized field; a stored-shape upgrade (a field App Fair
    // newly reads) re-imports so the new field fills in.
    let reimport = repo.synced_locale != locale || repo.synced_epoch != catalog::CATALOG_EPOCH;
    let fingerprint = repo.fingerprint.clone();
    // The primary host followed by any mirrors this repo declared, tried in order (#12).
    let bases = net::bases(address, &repo.mirrors);

    // entry: the current index pointer + available diffs. Verified via the signed entry.jar when a
    // fingerprint is pinned (#1); a pinned repo whose signature fails is an error, never a silent
    // unsigned fallback. Missing/garbled entry → a full (still hash-checked) download below.
    let entry: Option<Entry> = fetch_entry(&bases, &fingerprint)?;

    let up_to_date = entry
        .as_ref()
        .is_some_and(|e| e.timestamp == stored && stored != 0);

    // Nothing changed at all.
    if up_to_date && !reimport {
        state::set_sync(SyncUi::UpToDate);
        return Ok(());
    }

    // Only the locale or the stored shape changed: re-import the cached index — no download. If
    // there's no usable cache, this falls through to a full download below.
    if up_to_date
        && reimport
        && let Some(cached) = cache::read_index(address)
        && let Ok(index) = serde_json::from_slice::<IndexV2>(&cached)
    {
        return finish(repo.id, index, &locale, true);
    }

    // Obtain the new index (incremental diff when possible, else full), cache it, and apply.
    state::set_sync(SyncUi::Downloading(None));
    let progress = Progress::new();
    let (bytes, index) = obtain_index(
        address,
        &bases,
        index_file,
        entry.as_ref(),
        stored,
        &progress,
    )?;
    cache::write_index(address, &bytes);

    state::set_sync(SyncUi::Building);
    finish(repo.id, index, &locale, reimport)
}

/// Fetch the repo's entry: the verified signed `entry.jar` when a fingerprint is pinned (and this
/// platform can verify signatures), else the plain `entry.json`. A pinned repo whose signature
/// fails to verify is an error — we never fall back to an unverified index (#1). A missing/garbled
/// (but authenticated) entry parses to `None`, so the caller does a full download.
fn fetch_entry(bases: &[String], fingerprint: &str) -> Result<Option<Entry>, String> {
    if !fingerprint.is_empty() && crate::platform::verifies_signatures() {
        let jar =
            net::get_bytes_from(bases, "/entry.jar").map_err(|e| format!("entry.jar: {e}"))?;
        match crate::platform::verify_entry_jar(&jar, fingerprint) {
            Some(json) => Ok(serde_json::from_str::<Entry>(&json).ok()),
            None => Err("This repository's signature could not be verified.".to_string()),
        }
    } else {
        Ok(net::get_string_from(bases, "/entry.json")
            .ok()
            .and_then(|body| serde_json::from_str::<Entry>(&body).ok()))
    }
}

/// Sync an App Index or AltStore document: download it (from its address, then any mirror its
/// last import named), recognize its format, and import it when its change stamp moved or the
/// locale or stored shape changed. Neither format carries a reliable per-app change stamp, so a
/// changed document rewrites every app it lists; documents are small.
fn run_document(address: &str) -> Result<(), String> {
    state::set_sync(SyncUi::Checking);
    let owned = address.to_string();
    let repo = block_on(catalog::write(move |db| {
        let id = catalog::ensure_repo(db, &owned)?;
        catalog::repo(db, id)
    }))
    .map_err(|e| format!("db: {e}"))?
    .ok_or_else(|| "db: the catalog was removed".to_string())?;
    let locale = locale();
    let reimport = repo.synced_locale != locale || repo.synced_epoch != catalog::CATALOG_EPOCH;

    state::set_sync(SyncUi::Downloading(None));
    let progress = Progress::new();
    let bytes = download_with_progress(&net::bases(address, &repo.mirrors), "", 0, &progress)?;
    let digest = {
        let hex = sha256_hex(&bytes);
        i64::from_str_radix(&hex[..15], 16).unwrap_or(0)
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse: {e}"))?;
    let (kind, import) = if crate::appindex::sniff(&value) {
        let doc: crate::appindex::AppIndex =
            serde_json::from_value(value).map_err(|e| format!("parse: {e}"))?;
        let import = crate::appindex::import(&doc, repo.id, &locale, catalog::PLATFORM, digest);
        (format::APPINDEX, import)
    } else if crate::altstore::sniff(&value) {
        let source: crate::altstore::Source =
            serde_json::from_value(value).map_err(|e| format!("parse: {e}"))?;
        let import = crate::altstore::import(&source, repo.id, catalog::PLATFORM, digest);
        (format::ALTSTORE, import)
    } else if value.get("repo").is_some() && value.get("packages").is_some() {
        return Err(
            "This is an F-Droid index. Add its repository address, without index-v2.json."
                .to_string(),
        );
    } else {
        return Err("This is not an App Index, AltStore source, or F-Droid index.".to_string());
    };
    if import.stamp == repo.timestamp && repo.format == kind && !reimport {
        state::set_sync(SyncUi::UpToDate);
        return Ok(());
    }

    state::set_sync(SyncUi::Building);
    let id = repo.id;
    let locale_owned = locale.clone();
    let (changed, mirrors) = block_on(catalog::write(move |db| {
        catalog::set_repo_format(db, id, kind)?;
        let changed = catalog::import(db, id, import, &locale_owned, true)?;
        Ok((changed, catalog::mirror_map(db)?))
    }))
    .map_err(|e| format!("store: {e}"))?;
    imported(mirrors, changed);
    Ok(())
}

/// Import the index in one catalog transaction. The import records the locale and the
/// stored-shape epoch it ran under, so a later locale change or App Fair upgrade is detected.
fn finish(repo_id: u64, index: IndexV2, locale: &str, force: bool) -> Result<(), String> {
    let locale = locale.to_string();
    let (changed, mirrors) = block_on(catalog::write(move |db| {
        let changed = catalog::upsert_index(db, repo_id, &index, &locale, force)?;
        Ok((changed, catalog::mirror_map(db)?))
    }))
    .map_err(|e| format!("store: {e}"))?;
    imported(mirrors, changed);
    Ok(())
}

/// After an import commits: refresh the image fallback map, every page that reads the catalog,
/// and what the iOS marketplace extension knows.
fn imported(mirrors: Vec<(String, Vec<String>)>, changed: usize) {
    // Refresh the icon loader's mirror-fallback map, so icons and screenshots a metadata-only
    // repo doesn't host fall back to its mirrors (#12). Done before the catalog bump that
    // re-renders the list (and requests icons).
    crate::icons::set_mirror_bases(mirrors);
    state::bump_catalog();
    state::set_sync(SyncUi::Done(changed));
    crate::market::catalog_changed();
}

/// Get the current index bytes + parsed index. Prefers an incremental diff (`entry.diffs[stored]`)
/// applied to the cached index; on any problem — no cache, missing/corrupt diff, unparseable merge
/// — falls back to a full download.
fn obtain_index(
    address: &str,
    bases: &[String],
    index_file: &str,
    entry: Option<&Entry>,
    stored: i64,
    progress: &Progress,
) -> Result<(Vec<u8>, IndexV2), String> {
    // Prefer an incremental diff for our stored timestamp; on any problem, fall through to full.
    if stored != 0
        && let Some(diff_ref) = entry.and_then(|e| e.diffs.get(&stored.to_string()))
        && let Some(result) = try_incremental(address, bases, diff_ref)
    {
        return Ok(result);
    }

    // Full download of the current index.
    let index_name = entry
        .map(|e| e.index.name.clone())
        .unwrap_or_else(|| format!("/{index_file}"));
    let index_size = entry.map(|e| e.index.size.max(0) as u64).unwrap_or(0);
    let bytes = download_with_progress(bases, &index_name, index_size, progress)?;
    // When the entry came from a verified `entry.jar`, its index pointer carries a SHA-256 that
    // authenticates these bytes end to end (#1). Reject a mismatch rather than import tampered data.
    if let Some(expected) = entry.map(|e| e.index.sha256.as_str())
        && !expected.is_empty()
        && sha256_hex(&bytes) != expected
    {
        return Err("The downloaded catalog index failed its integrity check.".to_string());
    }
    let index = serde_json::from_slice(&bytes).map_err(|e| format!("parse: {e}"))?;
    Ok((bytes, index))
}

/// Try an incremental update: download the diff, verify it, and merge it onto the cached index.
/// Returns `None` (so the caller re-downloads) on any failure.
fn try_incremental(
    address: &str,
    bases: &[String],
    diff_ref: &FileRef,
) -> Option<(Vec<u8>, IndexV2)> {
    let cached = cache::read_index(address)?;
    let diff_bytes = net::get_bytes_from(bases, &diff_ref.name).ok()?;
    // Verify the DIFF's integrity. The merged index itself can't be hash-checked — our
    // re-serialization won't byte-match F-Droid's canonical JSON — so trust the RFC 7386 merge and
    // validate it by parsing below.
    if !diff_ref.sha256.is_empty() && sha256_hex(&diff_bytes) != diff_ref.sha256 {
        return None;
    }
    let mut base: serde_json::Value = serde_json::from_slice(&cached).ok()?;
    let patch: serde_json::Value = serde_json::from_slice(&diff_bytes).ok()?;
    cache::merge_patch(&mut base, &patch);
    let bytes = serde_json::to_vec(&base).ok()?;
    let index: IndexV2 = serde_json::from_slice(&bytes).ok()?;
    Some((bytes, index))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let mut s = String::with_capacity(64);
    for b in h.finalize() {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Blocking download that pushes `SyncUi::Downloading(fraction)` to the UI while it runs, trying
/// `{base}{path}` for each base until one succeeds (#12).
fn download_with_progress(
    bases: &[String],
    path: &str,
    fallback_total: u64,
    progress: &Progress,
) -> Result<Vec<u8>, String> {
    let finished = Arc::new(AtomicBool::new(false));
    let reporter = {
        let progress = progress.clone();
        let finished = finished.clone();
        thread::spawn(move || {
            while !finished.load(Ordering::Relaxed) {
                state::set_sync(SyncUi::Downloading(progress.fraction()));
                thread::sleep(Duration::from_millis(120));
            }
        })
    };
    let mut last = "no repository URL".to_string();
    let mut out = Err(String::new());
    for base in bases {
        match net::download(&format!("{base}{path}"), progress, fallback_total) {
            // The index isn't hash-checked here (the caller verifies it); drop the digest.
            Ok((bytes, _digest)) => {
                out = Ok(bytes);
                break;
            }
            Err(e) => last = e,
        }
    }
    finished.store(true, Ordering::Relaxed);
    let _ = reporter.join();
    out.map_err(|_| last)
}
