// Copyright © The Daybrite Project
// SPDX-License-Identifier: MPL-2.0

//! The on-device catalog (https://daybrite.dev/docs/reference/persistence): every catalog App
//! Fair knows, the apps each one lists, the localized category and anti-feature labels, and the
//! apps App Fair installed, in one SQLite file through day-persistence.
//!
//! One [`DatabaseWorker`] owns that file for the whole process. A catalog import writes
//! thousands of rows and a search reads hundreds, so neither runs on the UI thread: pages ask
//! through [`read`] and [`write`] and await the owned values that come back, and the sync
//! threads and the Android update worker block on the same queue with [`block_on`]. Being the
//! file's only reader and writer, the worker never has to look for changes made elsewhere.
//!
//! The functions below take the worker's [`ModelContainer`], so the tests call them directly
//! on an in-memory container. Every write ends in an explicit `save`, which inside a worker
//! write is part of that write's one transaction.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll, Wake, Waker};

use day::model::{ModelId, Source};
use day::prelude::*;
use day_persistence::prelude::*;
use day_persistence::{DatabaseWorker, DbError, ModelContainer, Sqlite, Value, WorkerRequest};

use crate::fdroid::{self, IndexV2, Package};
use crate::model::{AntiFeature, AppDetail, AppSummary, Category, RepoRow, SortOrder};

/// The catalog file under the app's data directory.
const FILE: &str = "catalog.db";

/// The rank stored for an app its catalog did not rank, and for every app in a catalog without
/// the `rank` extension: the largest value, so `ORDER BY rank` puts ranked apps first.
const UNRANKED: i64 = i64::MAX;

/// The version of the stored catalog's shape, bumped whenever App Fair starts reading a field it
/// used to ignore. A repo imported under an older epoch re-imports its cached index on the next
/// sync even when the catalog itself has not changed, so the new field fills in (`sync::run`).
pub const CATALOG_EPOCH: i64 = 1;

/// Rows the worker keeps resident per model. An import passes every app of a catalog through
/// the cache, and nothing holds them afterwards, so a small working set is enough.
const CACHE_ROWS: usize = 1024;

/// How many rows a browse or search returns.
pub const SEARCH_LIMIT: usize = 500;

// --- models --------------------------------------------------------------------------------

/// One catalog: an F-Droid-compatible repository.
#[derive(Model, Clone, Default, PartialEq, Debug)]
#[model(table = "repos")]
pub struct Repo {
    #[model(id)]
    pub id: u64,
    /// The base address, without the index file name: `https://f-droid.org/repo`.
    #[model(unique)]
    pub address: String,
    pub name: String,
    /// The index timestamp of the last import; `0` until the first sync.
    pub timestamp: i64,
    pub enabled: bool,
    /// The SHA-256 of the repository's signing certificate; empty for an unverified catalog.
    pub fingerprint: String,
    /// Which catalog wins when several list the same package: the highest.
    pub priority: i64,
    /// The mirror base URLs the index names, tried when the primary host fails (#12).
    #[model(json)]
    pub mirrors: Vec<String>,
    /// The locale the stored apps were picked in, so a locale change re-imports them.
    pub synced_locale: String,
    /// The [`CATALOG_EPOCH`] the stored apps were imported under.
    pub synced_epoch: i64,
    #[model(relation(target = App, inverse = "repo", delete = "cascade"))]
    pub apps: Many<App>,
    #[model(relation(target = CategoryLabel, inverse = "repo", delete = "cascade"))]
    pub categories: Many<CategoryLabel>,
    #[model(relation(target = AntiFeatureLabel, inverse = "repo", delete = "cascade"))]
    pub anti_features: Many<AntiFeatureLabel>,
}

/// One app as one catalog lists it, in one locale, at the newest version this device can run.
#[derive(Model, Clone, Default, PartialEq, Debug)]
#[model(table = "apps", index("repo", "pkg"))]
pub struct App {
    /// [`app_key`]: the catalog and the package, which is also the list row and route key.
    #[model(id)]
    pub id: String,
    pub repo: One<Repo>,
    /// The Android application id, `org.fdroid.fdroid`.
    #[model(index)]
    pub pkg: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    pub license: String,
    pub author: String,
    pub author_website: String,
    pub website: String,
    pub source_code: String,
    /// The repo-relative icon path, or empty.
    pub icon_path: String,
    #[model(json)]
    pub categories: Vec<String>,
    /// Repo-relative screenshot paths.
    #[model(json)]
    pub screenshots: Vec<String>,
    #[model(index)]
    pub last_updated: i64,
    pub version_name: String,
    pub version_code: i64,
    pub apk_path: String,
    pub apk_size: i64,
    pub apk_sha256: String,
    pub min_sdk: i64,
    /// Android permission names from the offered version's manifest.
    #[model(json)]
    pub permissions: Vec<String>,
    /// Anti-feature keys the offered version declares.
    #[model(json)]
    pub anti_features: Vec<String>,
    pub whats_new: String,
    /// The SHA-256 of the APK signing certificate the catalog promises (#3).
    pub signer: String,
    /// The app's place in the catalog's `rank` list, or [`UNRANKED`].
    #[model(index)]
    pub rank: i64,
}

impl App {
    /// The catalog this row belongs to.
    pub fn repo_id(&self) -> u64 {
        self.repo.id().map(ModelId::handle).unwrap_or_default()
    }
}

/// A catalog's localized name for one category key.
#[derive(Model, Clone, Default, PartialEq, Debug)]
#[model(table = "category_labels")]
pub struct CategoryLabel {
    /// `"{repo}~{key}"`.
    #[model(id)]
    pub id: String,
    pub repo: One<Repo>,
    pub key: String,
    pub name: String,
}

/// A catalog's localized name and explanation for one anti-feature key.
#[derive(Model, Clone, Default, PartialEq, Debug)]
#[model(table = "anti_feature_labels")]
pub struct AntiFeatureLabel {
    /// `"{repo}~{key}"`.
    #[model(id)]
    pub id: String,
    pub repo: One<Repo>,
    pub key: String,
    pub name: String,
    pub description: String,
}

/// An app App Fair installed, and the catalog it came from (#9).
#[derive(Model, Clone, Default, PartialEq, Debug)]
#[model(table = "installs")]
pub struct Install {
    #[model(id)]
    pub pkg: String,
    /// Not a relation: the record outlives the catalog's removal.
    pub repo: u64,
    /// Unix seconds.
    pub installed_at: i64,
}

/// The key of `pkg` in catalog `repo`.
pub fn app_key(repo: u64, pkg: &str) -> String {
    format!("{repo}~{pkg}")
}

/// The catalog and package an [`app_key`] names.
pub fn parse_app_key(key: &str) -> Option<(u64, &str)> {
    let (repo, pkg) = key.split_once('~')?;
    Some((repo.parse().ok()?, pkg))
}

// --- the worker ----------------------------------------------------------------------------

static WORKER: OnceLock<DatabaseWorker> = OnceLock::new();

/// The process's catalog worker, opened on first use. Opening runs on the worker's own thread,
/// but the first caller waits for it; `root()` calls this from a background thread at launch,
/// so the UI finds it open.
pub fn worker() -> &'static DatabaseWorker {
    WORKER.get_or_init(|| {
        let dir = crate::platform::data_dir();
        let path = dir.join(FILE);
        match block_on(DatabaseWorker::open(move || {
            std::fs::create_dir_all(&dir)
                .map_err(|e| DbError::driver(format!("creating {}: {e}", dir.display())))?;
            remove_diesel_catalog(&dir);
            open(Sqlite::at(&path))
        })) {
            Ok(worker) => worker,
            Err(e) => {
                error!("the catalog could not open ({e}); this session keeps it in memory");
                // An in-memory SQLite that cannot open is a broken build, not a runtime state.
                block_on(DatabaseWorker::open(|| open(Sqlite::memory())))
                    .expect("in-memory catalog")
            }
        }
    })
}

/// Run a read on the catalog worker.
pub fn read<R: Send + 'static>(
    f: impl FnOnce(&ModelContainer) -> Result<R, DbError> + Send + 'static,
) -> WorkerRequest<R> {
    worker().read(f)
}

/// Run one transaction on the catalog worker. It has committed when the request completes.
pub fn write<R: Send + 'static>(
    f: impl FnOnce(&ModelContainer) -> Result<R, DbError> + Send + 'static,
) -> WorkerRequest<R> {
    worker().write(f)
}

/// Wait for a future on a thread that has no executor: the sync threads and the Android update
/// worker, which block on the catalog worker's replies. Never call it on the UI thread.
pub fn block_on<F: Future>(f: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park();
    }
}

/// App Fair 1.0 kept its catalog in `catalog.db`, a schema this build does not read. The
/// catalog is a copy of what the repositories publish, so it syncs again rather than migrating.
fn remove_diesel_catalog(dir: &Path) {
    for name in ["catalog.db", "catalog.db-wal", "catalog.db-shm"] {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

/// Open the catalog in `driver`, with the default catalog present.
pub fn open(driver: Sqlite) -> Result<ModelContainer, DbError> {
    let db = ModelContainer::open(
        driver,
        schema![Repo, App, CategoryLabel, AntiFeatureLabel, Install],
    )?;
    db.set_cache_limit(CACHE_ROWS);
    // The default catalog exists from first launch (empty until it syncs), with the App Fair
    // repository's signing fingerprint pinned so its index is verified (#1).
    let id = ensure_repo(&db, &crate::model::split_index_url(DEFAULT_INDEX_URL).0)?;
    if repo(&db, id)?.is_some_and(|r| r.fingerprint.is_empty()) {
        set_repo_fingerprint(&db, id, DEFAULT_FINGERPRINT)?;
    }
    Ok(db)
}

/// The default catalog: the App Fair Index V2 repository.
pub const DEFAULT_INDEX_URL: &str = "https://appfair.net/repo/index-v2.json";

/// The App Fair repository's signing-certificate fingerprint (the SHA-256 `verifyEntryJar`
/// derives), so the default catalog's signed index is verified (#1).
const DEFAULT_FINGERPRINT: &str =
    "ddbbea5229957f75299c485d3b831ac30598459ad2ac69c293867163b4ed3c71";

/// One row's value, faulted in if it is not resident.
fn row<M: Model>(db: &ModelContainer, id: impl Into<ModelId<M>>) -> Result<Option<M>, DbError> {
    Ok(db
        .try_get::<M>(id)?
        .and_then(|e| e.with_value_untracked(|v| v.cloned())))
}

// --- repositories --------------------------------------------------------------------------

/// The id of the catalog at `address`, adding it when it is new. A new catalog gets the highest
/// priority, so a catalog added later wins over earlier ones for a package both list (#6).
pub fn ensure_repo(db: &ModelContainer, address: &str) -> Result<u64, DbError> {
    let repos = db.query::<Repo>().live().try_collect()?;
    if let Some(r) = repos.iter().find(|r| r.address == address) {
        return Ok(r.id);
    }
    let id = repos.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    db.insert(Repo {
        id,
        address: address.to_string(),
        enabled: true,
        priority: repos.iter().map(|r| r.priority).max().unwrap_or(0) + 1,
        ..Default::default()
    });
    db.save()?;
    Ok(id)
}

/// One catalog's row.
pub fn repo(db: &ModelContainer, id: u64) -> Result<Option<Repo>, DbError> {
    row::<Repo>(db, id)
}

/// A catalog's display name: its own name, else its address. Empty when the catalog is gone.
pub fn repo_name(db: &ModelContainer, id: u64) -> Result<String, DbError> {
    Ok(repo(db, id)?
        .map(|r| if r.name.is_empty() { r.address } else { r.name })
        .unwrap_or_default())
}

/// Pin a catalog's signing-certificate fingerprint.
pub fn set_repo_fingerprint(
    db: &ModelContainer,
    id: u64,
    fingerprint: &str,
) -> Result<(), DbError> {
    if let Some(r) = db.try_get::<Repo>(id)? {
        r.fingerprint().write(fingerprint.to_string());
    }
    db.save()
}

pub fn set_repo_enabled(db: &ModelContainer, id: u64, enabled: bool) -> Result<(), DbError> {
    if let Some(r) = db.try_get::<Repo>(id)? {
        r.enabled().write(enabled);
    }
    db.save()
}

/// Remove a catalog with its apps and labels (the relations cascade).
pub fn delete_repo(db: &ModelContainer, id: u64) -> Result<(), DbError> {
    db.delete::<Repo>(id)?;
    db.save()
}

/// Every catalog, highest priority first, with how many apps each lists.
pub fn repos(db: &ModelContainer) -> Result<Vec<RepoRow>, DbError> {
    db.query::<Repo>()
        .sort(Repo::priority().desc())
        .sort(Repo::id().asc())
        .live()
        .try_collect()?
        .into_iter()
        .map(|r| {
            let app_count = db
                .query::<App>()
                .filter(App::repo().is(r.id))
                .live_count()
                .try_get()?;
            Ok(RepoRow {
                id: r.id,
                address: r.address,
                name: r.name,
                timestamp: r.timestamp,
                enabled: r.enabled,
                app_count: app_count as i64,
                fingerprint: r.fingerprint,
                priority: r.priority,
            })
        })
        .collect()
}

/// Each catalog's address with its mirrors, primary first: the image loader's fallback map.
pub fn mirror_map(db: &ModelContainer) -> Result<Vec<(String, Vec<String>)>, DbError> {
    Ok(db
        .query::<Repo>()
        .live()
        .try_collect()?
        .into_iter()
        .map(|r| (r.address, r.mirrors))
        .collect())
}

// --- install provenance (#9) ---------------------------------------------------------------

/// Record that App Fair installed `pkg` from catalog `repo` at `installed_at` (Unix seconds).
pub fn record_install(
    db: &ModelContainer,
    pkg: &str,
    repo: u64,
    installed_at: i64,
) -> Result<(), DbError> {
    // Reinstalling replaces the record, so it is written as a fresh row.
    db.delete::<Install>(pkg)?;
    db.save()?;
    db.insert(Install {
        pkg: pkg.to_string(),
        repo,
        installed_at,
    });
    db.save()
}

/// The catalog App Fair installed `pkg` from, or `None` when App Fair did not install it.
pub fn install_source(db: &ModelContainer, pkg: &str) -> Result<Option<u64>, DbError> {
    Ok(row::<Install>(db, pkg)?.map(|i| i.repo))
}

/// Forget an install record (after an uninstall).
pub fn remove_install(db: &ModelContainer, pkg: &str) -> Result<(), DbError> {
    db.delete::<Install>(pkg)?;
    db.save()
}

// --- import --------------------------------------------------------------------------------

/// The flat row for one package in `locale`, at the newest version this device can run (#5),
/// or the newest overall when none can (the detail page then explains why).
fn build_app(repo: u64, pkg: &str, package: &Package, locale: &str, rank: i64) -> App {
    let meta = &package.metadata;
    let text = |field: &fdroid::Localized<String>| {
        fdroid::pick(field, locale).cloned().unwrap_or_default()
    };
    let mut app = App {
        id: app_key(repo, pkg),
        repo: One::to(repo),
        pkg: pkg.to_string(),
        name: fdroid::pick(&meta.name, locale)
            .cloned()
            .unwrap_or_else(|| pkg.to_string()),
        summary: text(&meta.summary),
        description: text(&meta.description),
        license: meta.license.clone(),
        author: meta.author_name.clone(),
        author_website: meta.author_website.clone(),
        website: meta.website.clone(),
        source_code: meta.source_code.clone(),
        icon_path: fdroid::pick(&meta.icon, locale)
            .map(|f| f.name.clone())
            .unwrap_or_default(),
        categories: meta.categories.clone(),
        screenshots: package.screenshot_paths(locale),
        last_updated: meta.last_updated,
        rank,
        ..Default::default()
    };
    let best = package.best_version_for(
        crate::platform::device_sdk(),
        &crate::platform::device_abis(),
    );
    if let Some(v) = best {
        app.version_name = v.manifest.version_name.clone();
        app.version_code = v.manifest.version_code;
        app.apk_path = v.file.name.clone();
        app.apk_size = v.file.size;
        app.apk_sha256 = v.file.sha256.clone();
        app.min_sdk = v.manifest.uses_sdk.min_sdk_version;
        app.permissions = v
            .manifest
            .uses_permission
            .iter()
            .map(|p| p.name.clone())
            .collect();
        app.anti_features = v.anti_features.keys().cloned().collect();
        app.whats_new = text(&v.whats_new);
        app.signer = v
            .manifest
            .signer
            .sha256
            .first()
            .cloned()
            .unwrap_or_default();
    }
    app
}

/// The `(last_updated, rank)` stamp of every app catalog `repo` holds, by package. Read as raw
/// SQL, the import tier, so a catalog of thousands of apps is not faulted in to compare two
/// numbers per row.
fn stamps(db: &ModelContainer, repo: u64) -> Result<HashMap<String, (i64, i64)>, DbError> {
    db.try_with_connection(|conn| {
        let mut out = HashMap::new();
        conn.query(
            "SELECT pkg, last_updated, rank FROM apps WHERE repo = ?",
            &[Value::Int(repo as i64)],
            &mut |row| {
                if let (Ok(pkg), Ok(updated), Ok(rank)) = (
                    row.get(0).as_text(),
                    row.get(1).as_int(),
                    row.get(2).as_int(),
                ) {
                    out.insert(pkg.to_string(), (updated, rank));
                }
            },
        )?;
        Ok(out)
    })
}

/// Apply a freshly parsed index to catalog `repo`, writing only the apps that changed, and
/// return how many it wrote.
///
/// An app's `(last_updated, rank)` is its change stamp: a stored app with the same stamp is
/// left alone unless `force` (a locale change, so every localized field is picked again). Rank
/// is part of the stamp because a catalog can reorder its ranking without touching
/// `lastUpdated`. Apps the index no longer lists are deleted, and the category and
/// anti-feature labels are replaced. Run inside one worker write, the whole import is one
/// transaction.
pub fn upsert_index(
    db: &ModelContainer,
    repo: u64,
    index: &IndexV2,
    locale: &str,
    force: bool,
) -> Result<usize, DbError> {
    let ranks: HashMap<&str, i64> = index
        .rank
        .iter()
        .enumerate()
        .map(|(i, p)| (p.as_str(), i as i64))
        .collect();
    let stored = stamps(db, repo)?;

    // A changed app is written whole. Inserting a key the file already holds is an upsert,
    // so only a resident copy has to leave the cache first.
    let apps = db.cache::<App>();
    let mut written = 0;
    for (pkg, package) in &index.packages {
        let rank = ranks.get(pkg.as_str()).copied().unwrap_or(UNRANKED);
        if !force && stored.get(pkg) == Some(&(package.metadata.last_updated, rank)) {
            continue;
        }
        let app = build_app(repo, pkg, package, locale, rank);
        apps.depopulate(app.id.as_str());
        db.insert(app);
        written += 1;
    }
    for pkg in stored.keys().filter(|p| !index.packages.contains_key(*p)) {
        db.delete::<App>(app_key(repo, pkg))?;
    }

    replace_labels(
        db,
        repo,
        index
            .repo
            .categories
            .iter()
            .map(|(key, def)| CategoryLabel {
                id: app_key(repo, key),
                repo: One::to(repo),
                key: key.clone(),
                name: fdroid::pick(&def.name, locale)
                    .cloned()
                    .unwrap_or_else(|| key.clone()),
            }),
        |l| &l.id,
    )?;
    replace_labels(
        db,
        repo,
        index
            .repo
            .anti_features
            .iter()
            .map(|(key, def)| AntiFeatureLabel {
                id: app_key(repo, key),
                repo: One::to(repo),
                key: key.clone(),
                name: fdroid::pick(&def.name, locale)
                    .cloned()
                    .unwrap_or_else(|| key.clone()),
                description: fdroid::pick(&def.description, locale)
                    .cloned()
                    .unwrap_or_default(),
            }),
        |l| &l.id,
    )?;

    if let Some(r) = db.try_get::<Repo>(repo)? {
        r.timestamp().write(index.repo.timestamp);
        r.name().write(
            fdroid::pick(&index.repo.name, locale)
                .cloned()
                .unwrap_or_default(),
        );
        r.mirrors().write(
            index
                .repo
                .mirrors
                .iter()
                .map(|m| m.url.trim_end_matches('/').to_string())
                .filter(|u| !u.is_empty())
                .collect(),
        );
        r.synced_locale().write(locale.to_string());
        r.synced_epoch().write(CATALOG_EPOCH);
    }
    db.save()?;
    Ok(written)
}

/// Replace one catalog's labels of model `M` with `labels`: upsert each, delete the rest.
fn replace_labels<M: Model>(
    db: &ModelContainer,
    repo: u64,
    labels: impl Iterator<Item = M>,
    id: impl Fn(&M) -> &String,
) -> Result<(), DbError> {
    let existing: Vec<String> = db.try_with_connection(|conn| {
        let mut out = Vec::new();
        conn.query(
            &format!("SELECT id FROM {} WHERE repo = ?", M::TABLE),
            &[Value::Int(repo as i64)],
            &mut |row| {
                if let Ok(id) = row.get(0).as_text() {
                    out.push(id.to_string());
                }
            },
        )?;
        Ok(out)
    })?;
    let cache = db.cache::<M>();
    let mut kept = HashSet::new();
    for label in labels {
        kept.insert(id(&label).clone());
        cache.depopulate(id(&label).as_str());
        db.insert(label);
    }
    for gone in existing.iter().filter(|g| !kept.contains(*g)) {
        db.delete::<M>(gone.as_str())?;
    }
    Ok(())
}

// --- queries -------------------------------------------------------------------------------

/// What a browse or search asks for: each field is one control on the Catalogs page.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Search {
    pub query: String,
    pub category: Option<String>,
    /// One catalog, or `None` for every enabled catalog with each package once (#6).
    pub repo: Option<u64>,
    /// Hide apps carrying any of these anti-feature keys (#13).
    pub exclude_anti: Vec<String>,
    pub sort: SortOrder,
    /// The device locale, for the [`SortOrder::Name`] collation.
    pub locale: String,
}

/// The apps matching `s`, in its order, at most [`SEARCH_LIMIT`].
///
/// This is the one query that needs raw SQL: keeping one row per package across catalogs
/// compares rows with each other, and search relevance orders by expressions over the search
/// text. `day_fold` is the container's Unicode case fold, so `É` matches `é`.
pub fn search(db: &ModelContainer, s: &Search) -> Result<Vec<AppSummary>, DbError> {
    let q = s.query.trim().to_lowercase();
    let mut sql =
        String::from("SELECT a.id FROM apps a JOIN repos r ON r.id = a.repo WHERE r.enabled = 1");
    let mut params = Vec::new();
    if !q.is_empty() {
        sql.push_str(
            " AND (instr(day_fold(a.name), ?) > 0 OR instr(day_fold(a.summary), ?) > 0 \
             OR instr(day_fold(a.pkg), ?) > 0)",
        );
        params.extend(std::iter::repeat_n(Value::Text(q.clone()), 3));
    }
    if let Some(cat) = &s.category {
        sql.push_str(" AND EXISTS (SELECT 1 FROM json_each(a.categories) WHERE value = ?)");
        params.push(Value::Text(cat.clone()));
    }
    match s.repo {
        Some(repo) => {
            sql.push_str(" AND a.repo = ?");
            params.push(Value::Int(repo as i64));
        }
        // Every catalog: keep the winning row per package (the highest catalog priority, then
        // the highest version code, then the lowest catalog id).
        None => sql.push_str(
            " AND NOT EXISTS (SELECT 1 FROM apps a2 JOIN repos r2 ON r2.id = a2.repo \
             WHERE r2.enabled = 1 AND a2.pkg = a.pkg AND (r2.priority > r.priority \
             OR (r2.priority = r.priority AND a2.version_code > a.version_code) \
             OR (r2.priority = r.priority AND a2.version_code = a.version_code \
             AND a2.repo < a.repo)))",
        ),
    }
    if !s.exclude_anti.is_empty() {
        let marks = vec!["?"; s.exclude_anti.len()].join(", ");
        sql.push_str(&format!(
            " AND NOT EXISTS (SELECT 1 FROM json_each(a.anti_features) WHERE value IN ({marks}))"
        ));
        params.extend(s.exclude_anti.iter().cloned().map(Value::Text));
    }
    // Default browsing follows the catalog's rank, then recency; a search puts relevance first
    // (exact name, name prefix, name match, summary match). Name is collated in Rust below and
    // ordered by stored name here only so the window is stable.
    sql.push_str(match s.sort {
        SortOrder::LastUpdated => " ORDER BY a.last_updated DESC",
        SortOrder::Name => " ORDER BY a.name",
        SortOrder::Default if q.is_empty() => " ORDER BY a.rank, a.last_updated DESC",
        SortOrder::Default => {
            " ORDER BY day_fold(a.name) = ? DESC, instr(day_fold(a.name), ?) = 1 DESC, \
             instr(day_fold(a.name), ?) > 0 DESC, instr(day_fold(a.summary), ?) > 0 DESC, \
             a.rank, a.last_updated DESC"
        }
    });
    if s.sort == SortOrder::Default && !q.is_empty() {
        params.extend(std::iter::repeat_n(Value::Text(q), 4));
    }
    sql.push_str(&format!(", a.id LIMIT {SEARCH_LIMIT}"));

    let apps = db
        .query_raw::<App>(sql, params, &["apps", "repos"])
        .try_collect()?;
    let addresses = addresses(db)?;
    let mut out: Vec<AppSummary> = apps.iter().map(|a| summary(a, &addresses)).collect();
    if s.sort == SortOrder::Name {
        // Bundled SQLite cannot collate for a locale, so the order comes from ICU4X here.
        crate::collate::sort_by_name(&s.locale, &mut out, |a| a.name.as_str());
    }
    Ok(out)
}

/// Each catalog's base address, by id.
fn addresses(db: &ModelContainer) -> Result<HashMap<u64, String>, DbError> {
    Ok(db
        .query::<Repo>()
        .live()
        .try_collect()?
        .into_iter()
        .map(|r| (r.id, r.address))
        .collect())
}

fn summary(a: &App, addresses: &HashMap<u64, String>) -> AppSummary {
    AppSummary {
        repo_id: a.repo_id(),
        pkg: a.pkg.clone(),
        name: a.name.clone(),
        summary: a.summary.clone(),
        icon_path: a.icon_path.clone(),
        repo_address: addresses.get(&a.repo_id()).cloned().unwrap_or_default(),
        version_name: a.version_name.clone(),
        version_code: a.version_code,
    }
}

/// The catalog row for `pkg` that wins across the enabled catalogs (#6): the highest catalog
/// priority, then the highest version code. For the Updates tab.
pub fn summary_by_pkg(db: &ModelContainer, pkg: &str) -> Result<Option<AppSummary>, DbError> {
    let repos: HashMap<u64, Repo> = db
        .query::<Repo>()
        .filter(Repo::enabled().eq(true))
        .live()
        .try_collect()?
        .into_iter()
        .map(|r| (r.id, r))
        .collect();
    let best = db
        .query::<App>()
        .filter(App::pkg().eq(pkg.to_string()))
        .live()
        .try_collect()?
        .into_iter()
        .filter_map(|a| Some((repos.get(&a.repo_id())?.priority, a)))
        .max_by_key(|(priority, a)| (*priority, a.version_code));
    let addresses = repos.into_iter().map(|(id, r)| (id, r.address)).collect();
    Ok(best.map(|(_, a)| summary(&a, &addresses)))
}

/// The categories of the enabled catalogs, one per key, by name.
pub fn categories(db: &ModelContainer) -> Result<Vec<Category>, DbError> {
    let mut seen = HashSet::new();
    let mut out: Vec<Category> = db
        .query::<CategoryLabel>()
        .filter(CategoryLabel::repo().any(Repo::enabled().eq(true)))
        .sort(CategoryLabel::name().asc())
        .live()
        .try_collect()?
        .into_iter()
        .filter(|c| seen.insert(c.key.clone()))
        .map(|c| Category {
            key: c.key,
            name: c.name,
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Everything the detail page shows about one app.
pub fn detail(db: &ModelContainer, repo_id: u64, pkg: &str) -> Result<Option<AppDetail>, DbError> {
    let Some(a) = row::<App>(db, app_key(repo_id, pkg))? else {
        return Ok(None);
    };
    let Some(r) = repo(db, repo_id)? else {
        return Ok(None);
    };
    let anti_features = a
        .anti_features
        .iter()
        .map(|key| {
            let label = row::<AntiFeatureLabel>(db, app_key(repo_id, key))?;
            Ok(AntiFeature {
                key: key.clone(),
                name: label
                    .as_ref()
                    .map_or_else(|| key.clone(), |l| l.name.clone()),
                description: label.map(|l| l.description).unwrap_or_default(),
            })
        })
        .collect::<Result<_, DbError>>()?;
    Ok(Some(AppDetail {
        repo_id,
        pkg: a.pkg,
        name: a.name,
        summary: a.summary,
        description: a.description,
        license: a.license,
        author: a.author,
        author_website: a.author_website,
        website: a.website,
        source_code: a.source_code,
        icon_path: a.icon_path,
        repo_address: r.address,
        repo_mirrors: r.mirrors,
        screenshots: a.screenshots,
        version_name: a.version_name,
        version_code: a.version_code,
        apk_path: a.apk_path,
        apk_size: a.apk_size,
        apk_sha256: a.apk_sha256,
        signer: a.signer,
        min_sdk: a.min_sdk,
        permissions: a.permissions,
        anti_features,
        whats_new: a.whats_new,
        last_updated: a.last_updated,
        categories: a.categories,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A small F-Droid-compatible catalog and a diff onto it, to exercise the store and the
    // incremental update with no network.
    const BASE: &str = include_str!("testdata/index-v2.json");
    const DIFF: &str = include_str!("testdata/diff-1000.json");

    const ADDR: &str = "https://test.example/repo";

    fn memory() -> ModelContainer {
        open(Sqlite::memory()).expect("in-memory catalog")
    }

    fn setup(locale: &str) -> (ModelContainer, u64) {
        let db = memory();
        let repo = ensure_repo(&db, ADDR).unwrap();
        let index: IndexV2 = serde_json::from_str(BASE).unwrap();
        upsert_index(&db, repo, &index, locale, false).unwrap();
        (db, repo)
    }

    fn browse(db: &ModelContainer, f: impl FnOnce(&mut Search)) -> Vec<AppSummary> {
        let mut s = Search {
            locale: "en-US".into(),
            ..Default::default()
        };
        f(&mut s);
        search(db, &s).unwrap()
    }

    fn pkgs(v: &[AppSummary]) -> Vec<&str> {
        v.iter().map(|a| a.pkg.as_str()).collect()
    }

    #[test]
    fn the_default_catalog_exists_from_the_start() {
        let db = memory();
        let r = repos(&db).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].address, "https://appfair.net/repo");
        assert_eq!(r[0].fingerprint, DEFAULT_FINGERPRINT);
        assert!(r[0].enabled);
    }

    #[test]
    fn upsert_and_query() {
        let (db, repo) = setup("en-US");

        let r: Vec<RepoRow> = repos(&db)
            .unwrap()
            .into_iter()
            .filter(|r| r.id == repo)
            .collect();
        assert_eq!(r[0].app_count, 2);
        assert_eq!(r[0].timestamp, 1000);
        assert_eq!(r[0].name, "Test Repo");
        // Added after the default catalog, so it wins a package both list (#6).
        assert!(r[0].priority > 1);

        assert_eq!(browse(&db, |_| {}).len(), 2);
        let hits = browse(&db, |s| s.query = "Alpha".into());
        assert_eq!(hits[0].pkg, "com.example.alpha");
        // Case-insensitive, and matching the package id too.
        assert_eq!(
            pkgs(&browse(&db, |s| s.query = "BETA".into())),
            ["com.example.beta"]
        );
        assert_eq!(
            pkgs(&browse(&db, |s| s.query = "example.alpha".into())),
            ["com.example.alpha"]
        );

        // Category filter over the stored JSON list.
        let sys = browse(&db, |s| s.category = Some("System".into()));
        assert_eq!(pkgs(&sys), ["com.example.beta"]);

        // Anti-feature exclusion (#13): hiding "Tracking" drops alpha.
        let filtered = browse(&db, |s| s.exclude_anti = vec!["Tracking".into()]);
        assert_eq!(pkgs(&filtered), ["com.example.beta"]);

        // A disabled catalog leaves browsing.
        set_repo_enabled(&db, repo, false).unwrap();
        assert!(browse(&db, |_| {}).is_empty());
        set_repo_enabled(&db, repo, true).unwrap();

        let names: Vec<String> = categories(&db)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert!(names.contains(&"Internet".to_string()));
        assert!(names.contains(&"System".to_string()));

        // Detail: authors, list fields, and localized anti-features all round-trip.
        let d = detail(&db, repo, "com.example.alpha").unwrap().unwrap();
        assert_eq!(d.author, "Alice");
        assert_eq!(d.author_website, "https://alice.example");
        assert_eq!(d.description, "Alpha description");
        assert_eq!(d.apk_size, 111);
        assert_eq!(d.signer, "deadbeefcafe0001"); // #3 signer pinning
        assert_eq!(d.min_sdk, 21);
        assert_eq!(d.permissions, ["android.permission.INTERNET"]);
        assert_eq!(d.categories, ["Internet"]);
        assert_eq!(d.anti_features.len(), 1);
        assert_eq!(d.anti_features[0].name, "Tracking");
        assert_eq!(d.anti_features[0].description, "Tracks you.");
        assert_eq!(d.repo_address, ADDR);

        assert!(summary_by_pkg(&db, "com.example.beta").unwrap().is_some());
    }

    #[test]
    fn a_package_in_two_catalogs_shows_once() {
        let (db, first) = setup("en-US");
        let index: IndexV2 = serde_json::from_str(BASE).unwrap();
        let second = ensure_repo(&db, "https://second.example/repo").unwrap();
        upsert_index(&db, second, &index, "en-US", false).unwrap();

        // The catalog added later has the higher priority, so its rows win.
        let all = browse(&db, |_| {});
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|a| a.repo_id == second));
        assert_eq!(
            summary_by_pkg(&db, "com.example.alpha")
                .unwrap()
                .unwrap()
                .repo_id,
            second
        );
        // One catalog at a time shows all of its own.
        assert_eq!(browse(&db, |s| s.repo = Some(first)).len(), 2);

        // Removing a catalog takes its apps and labels with it.
        delete_repo(&db, second).unwrap();
        assert!(browse(&db, |_| {}).iter().all(|a| a.repo_id == first));
        assert!(repos(&db).unwrap().iter().all(|r| r.id != second));
        assert!(detail(&db, second, "com.example.alpha").unwrap().is_none());
    }

    #[test]
    fn sort_orders() {
        // The test data ranks beta above alpha, the reverse of recency (alpha 100 > beta 90)
        // and of name order, so each sort gives a different answer.
        let (db, _) = setup("en-US");
        assert_eq!(
            pkgs(&browse(&db, |_| {})),
            ["com.example.beta", "com.example.alpha"]
        );
        assert_eq!(
            pkgs(&browse(&db, |s| s.sort = SortOrder::LastUpdated)),
            ["com.example.alpha", "com.example.beta"]
        );
        assert_eq!(
            browse(&db, |s| s.sort = SortOrder::Name)
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            ["Alpha", "Beta"]
        );
    }

    #[test]
    fn stores_mirrors_from_index() {
        // The mirror base URLs, trailing slash trimmed, for failover.
        let (db, repo) = setup("en-US");
        assert_eq!(
            super::repo(&db, repo).unwrap().unwrap().mirrors,
            [
                "https://mirror1.example/repo",
                "https://mirror2.example/repo"
            ]
        );
        let map = mirror_map(&db).unwrap();
        assert!(map.iter().any(|(a, m)| a == ADDR && m.len() == 2));
    }

    #[test]
    fn install_provenance_round_trips() {
        // record → source lookup → name → forget (#9).
        let (db, repo) = setup("en-US");
        assert!(install_source(&db, "com.example.alpha").unwrap().is_none());

        record_install(&db, "com.example.alpha", repo, 1_700_000_000).unwrap();
        record_install(&db, "com.example.alpha", repo, 1_700_000_100).unwrap();
        assert_eq!(
            install_source(&db, "com.example.alpha").unwrap(),
            Some(repo)
        );
        assert_eq!(repo_name(&db, repo).unwrap(), "Test Repo");

        remove_install(&db, "com.example.alpha").unwrap();
        assert!(install_source(&db, "com.example.alpha").unwrap().is_none());
    }

    #[test]
    fn picks_requested_locale() {
        let (db, repo) = setup("fr");
        let d = detail(&db, repo, "com.example.alpha").unwrap().unwrap();
        assert_eq!(d.name, "Alpha FR");
        assert_eq!(d.summary, "Résumé Alpha");
        assert_eq!(d.description, "Description Alpha");
        assert_eq!(d.anti_features[0].name, "Pistage"); // a localized anti-feature
        assert_eq!(super::repo(&db, repo).unwrap().unwrap().synced_locale, "fr");
    }

    #[test]
    fn unchanged_apps_are_not_rewritten() {
        let (db, repo) = setup("en-US");
        let index: IndexV2 = serde_json::from_str(BASE).unwrap();
        assert_eq!(upsert_index(&db, repo, &index, "en-US", false).unwrap(), 0);
        // A forced re-import (a locale change) writes every app again, in the new locale.
        assert_eq!(upsert_index(&db, repo, &index, "fr", true).unwrap(), 2);
        assert_eq!(
            detail(&db, repo, "com.example.alpha")
                .unwrap()
                .unwrap()
                .name,
            "Alpha FR"
        );
        assert_eq!(categories(&db).unwrap().len(), 2);
    }

    #[test]
    fn incremental_diff_merge_and_apply() {
        // The incremental-update core: merge the diff onto the base index (RFC 7386).
        let mut base: serde_json::Value = serde_json::from_str(BASE).unwrap();
        let patch: serde_json::Value = serde_json::from_str(DIFF).unwrap();
        crate::cache::merge_patch(&mut base, &patch);
        let merged: IndexV2 = serde_json::from_value(base).unwrap();
        assert_eq!(merged.repo.timestamp, 2000);
        assert!(merged.packages.contains_key("com.example.gamma"));
        assert!(!merged.packages.contains_key("com.example.beta"));

        // Apply base, then the merged index; the incremental result must match.
        let (db, repo) = setup("en-US");
        upsert_index(&db, repo, &merged, "en-US", false).unwrap();

        assert!(detail(&db, repo, "com.example.beta").unwrap().is_none());
        assert_eq!(
            detail(&db, repo, "com.example.gamma")
                .unwrap()
                .unwrap()
                .author,
            "Carol"
        );
        assert_eq!(
            detail(&db, repo, "com.example.alpha")
                .unwrap()
                .unwrap()
                .description,
            "Alpha UPDATED"
        );
        let r = repos(&db).unwrap();
        let r = r.iter().find(|r| r.id == repo).unwrap();
        assert_eq!(r.timestamp, 2000);
        assert_eq!(r.app_count, 2); // alpha + gamma
    }

    #[test]
    fn the_worker_answers_reads_after_its_writes() {
        let worker = block_on(DatabaseWorker::open(|| open(Sqlite::memory()))).unwrap();
        let repo = block_on(worker.write(|db| {
            let repo = ensure_repo(db, ADDR)?;
            let index: IndexV2 = serde_json::from_str(BASE).unwrap();
            upsert_index(db, repo, &index, "en-US", false)?;
            Ok(repo)
        }))
        .unwrap();
        let hits = block_on(worker.read(|db| {
            search(
                db,
                &Search {
                    query: "alpha".into(),
                    locale: "en-US".into(),
                    ..Default::default()
                },
            )
        }))
        .unwrap();
        assert_eq!(pkgs(&hits), ["com.example.alpha"]);
        assert_eq!(hits[0].repo_id, repo);
        block_on(worker.close()).unwrap();
    }
}
