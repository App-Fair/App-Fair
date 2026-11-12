//! Download-and-install orchestration. Downloads the APK (progress + cancel) to a temp file, then
//! hands it to Android's PackageInstaller through the `platform` bridge. The install result comes
//! back asynchronously via the JNI callback in `platform`, which updates the install sheet.

use day::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::model::AppDetail;
use crate::net::{self, Progress};
use crate::state::{self, InstallPhase, InstallUi};

/// The current download's cancel handle, so the sheet's Cancel button can stop it. App-wide:
/// App Fair installs one app at a time, whichever window started it.
#[derive(Clone, Default)]
struct Cancel(std::rc::Rc<std::cell::RefCell<Option<Progress>>>);

impl Ambient for Cancel {
    fn create() -> Self {
        Cancel::default()
    }
}

impl Cancel {
    /// The same shape the `thread_local!` accessor had, so every call site below reads the same.
    fn with<R>(f: impl FnOnce(&std::cell::RefCell<Option<Progress>>) -> R) -> R {
        f(&Cancel::app().0)
    }
}

/// Cancel a download in progress (no effect once the system installer has taken over).
pub fn cancel() {
    if crate::market::enabled() {
        crate::market::cancel();
        return;
    }
    Cancel::with(|c| {
        if let Some(p) = c.borrow().as_ref() {
            p.cancel();
        }
    });
}

/// Delete leftover staged `.apk` files in the data directory. Per-install cleanup removes each APK
/// once its install finishes, but a process killed mid-download (or before the terminal install
/// result arrives) leaves one behind. Called once at launch, when nothing can be in flight — the
/// installer copies the APK into its own session, so no live session references these files.
pub fn prune_stale_apks() {
    let dir = crate::platform::data_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("apk"))
        {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Begin downloading and installing `app`.
pub fn start(app: &AppDetail) {
    // Failover targets for the APK: the app's repo first, then its mirrors (#12).
    let bases = net::bases(&app.repo_address, &app.repo_mirrors);
    let apk_path = app.apk_path.clone();
    let total = app.apk_size.max(0) as u64;
    let pkg = app.pkg.clone();
    let name = if app.name.is_empty() {
        app.pkg.clone()
    } else {
        app.name.clone()
    };
    let apk_file =
        crate::platform::data_dir().join(format!("{}-{}.apk", app.pkg, app.version_code));
    // The catalog's SHA-256 for this APK (lowercase hex), verified after download.
    let expected_sha = app.apk_sha256.trim().to_ascii_lowercase();
    // The catalog's pinned signing-cert SHA-256, verified against the APK's actual signer (#3).
    let expected_signer = app.signer.trim().to_ascii_lowercase();

    state::set_in_flight(&pkg, &name, app.repo_id, &apk_file);
    state::set_install(Some(InstallUi {
        pkg: pkg.clone(),
        name: name.clone(),
        phase: InstallPhase::Downloading,
        progress: 0.0,
        downloaded: 0,
        total,
    }));

    let progress = Progress::new();
    Cancel::with(|c| *c.borrow_mut() = Some(progress.clone()));

    thread::spawn(move || {
        let finished = Arc::new(AtomicBool::new(false));
        // Report download progress to the sheet while the download runs.
        let reporter = {
            let progress = progress.clone();
            let finished = finished.clone();
            let (pkg, name) = (pkg.clone(), name.clone());
            thread::spawn(move || {
                while !finished.load(Ordering::Relaxed) {
                    state::set_install(Some(InstallUi {
                        pkg: pkg.clone(),
                        name: name.clone(),
                        phase: InstallPhase::Downloading,
                        progress: progress.fraction().unwrap_or(0.0),
                        downloaded: progress.downloaded(),
                        total: progress.total(),
                    }));
                    thread::sleep(Duration::from_millis(100));
                }
            })
        };

        // Stream the APK straight to disk so even a 500 MB download never sits in memory, resuming
        // a partial file and failing over to mirrors. The running SHA-256 comes back from the
        // stream; the file is only handed to the installer once it verifies.
        let result = download_apk(&bases, &apk_path, &apk_file, &progress, total);
        finished.store(true, Ordering::Relaxed);
        let _ = reporter.join();

        match result {
            Err(e) if e == "cancelled" => {
                // Discard the partial download.
                let _ = std::fs::remove_file(&apk_file);
                state::set_install(Some(terminal(&pkg, &name, InstallPhase::Cancelled)));
            }
            Err(e) => {
                // Keep the partial file on a network failure so a later retry resumes it (#11);
                // launch-time pruning clears it if the user never comes back.
                state::set_install(Some(terminal(&pkg, &name, InstallPhase::Failed(e))));
            }
            Ok(digest) => {
                // Verify the SHA-256 against the catalog before the installer ever sees the file.
                // An empty catalog hash means the repo didn't publish one; skip the check rather
                // than block the install, but a present hash must match exactly.
                if !expected_sha.is_empty() && digest != expected_sha {
                    let _ = std::fs::remove_file(&apk_file);
                    state::set_install(Some(terminal(
                        &pkg,
                        &name,
                        InstallPhase::Failed(
                            "Checksum verification failed — the download does not match the \
                             catalog and was not installed."
                                .to_string(),
                        ),
                    )));
                    return;
                }
                // Verify the APK's signing certificate matches the catalog's pinned signer (#3), so
                // a hash-matching but differently-signed APK (e.g. from a compromised mirror) can't
                // install. An empty pinned signer means the catalog published none; skip then.
                if !expected_signer.is_empty() {
                    let actual = crate::platform::apk_signer_sha256(&apk_file.to_string_lossy());
                    if !actual.is_empty() && actual.to_ascii_lowercase() != expected_signer {
                        let _ = std::fs::remove_file(&apk_file);
                        state::set_install(Some(terminal(
                            &pkg,
                            &name,
                            InstallPhase::Failed(
                                "Signature verification failed — this APK is signed by a different \
                                 key than the catalog lists and was not installed."
                                    .to_string(),
                            ),
                        )));
                        return;
                    }
                }
                // Move to the Installing phase; the system reads the file on disk and reports via
                // JNI. `total` is the catalog's declared size (fallback if the server omitted one).
                let size = std::fs::metadata(&apk_file)
                    .map(|m| m.len())
                    .unwrap_or(total.max(progress.downloaded()));
                state::set_install(Some(InstallUi {
                    pkg: pkg.clone(),
                    name: name.clone(),
                    phase: InstallPhase::Installing,
                    progress: 1.0,
                    downloaded: size,
                    total: size,
                }));
                crate::platform::install_apk(&apk_file.to_string_lossy(), &name);
            }
        }
    });
}

/// Download the APK to `file`, resuming any partial and failing over across `bases` (#11, #12).
/// Returns the file's SHA-256, or the last host's error (`"cancelled"` short-circuits).
fn download_apk(
    bases: &[String],
    apk_path: &str,
    file: &std::path::Path,
    progress: &Progress,
    total: u64,
) -> Result<String, String> {
    let mut last = "no repository URL".to_string();
    for base in bases {
        match net::download_to_file_resumable(&format!("{base}{apk_path}"), file, progress, total) {
            Ok(digest) => return Ok(digest),
            Err(e) if e == "cancelled" => return Err(e),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn terminal(pkg: &str, name: &str, phase: InstallPhase) -> InstallUi {
    InstallUi {
        pkg: pkg.to_string(),
        name: name.to_string(),
        phase,
        progress: 1.0,
        downloaded: 0,
        total: 0,
    }
}
