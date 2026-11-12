//! App-wide state: the reactive signals the pages read, the snapshots of catalog reads they
//! show, and the `Setter`s background work uses to push results back. App-wide signals live in
//! the root scope so they outlive the page that first reads them; the setters are stored
//! globally because a download thread or the install callback needs them from off the main
//! thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use day::prelude::*;
use day::reactive::Resource;
use day_persistence::{DbError, ModelContainer};

use crate::catalog;
use crate::model::{AppDetail, AppSummary, Category, RepoRow, SortOrder};
use crate::platform;

/// Remaining apps in a "Update all" batch — each installs after the previous reaches a terminal
/// state (#8). Empty for a normal single install, so it never affects that path.
static UPDATE_QUEUE: Mutex<Vec<AppDetail>> = Mutex::new(Vec::new());

/// Progress of a catalog sync, shown in the list header.
#[derive(Clone, Debug, PartialEq)]
pub enum SyncUi {
    Idle,
    Checking,
    /// Download fraction, or `None` when the size is unknown.
    Downloading(Option<f32>),
    Building,
    UpToDate,
    Done(usize),
    Error(String),
}

/// Progress of an app download + install, shown in the install sheet.
#[derive(Clone, Debug)]
pub struct InstallUi {
    /// The package being installed (identifies the in-flight app; not shown directly).
    #[allow(dead_code)]
    pub pkg: String,
    pub name: String,
    pub phase: InstallPhase,
    pub progress: f32,
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InstallPhase {
    /// iOS: the marketplace server is being asked to authorize the install.
    Authorizing,
    Downloading,
    Installing,
    Success,
    Failed(String),
    Cancelled,
}

/// What ONE WINDOW is browsing (docs/state.md): its search text, its category and catalog
/// filters, and its sort order. Per-window because that is what a second window is for — one
/// showing Games while another searches, each keeping its own place.
#[derive(Clone, Copy)]
pub struct Scene {
    query: Signal<String>,
    category: Signal<Option<String>>,
    /// The catalog to browse (`None` = all enabled catalogs).
    repo_filter: Signal<Option<u64>>,
    /// The catalog list's sort order (the sort drop-down).
    sort: Signal<SortOrder>,
}

impl Ambient for Scene {
    fn create() -> Self {
        Scene {
            query: Signal::new(String::new()),
            category: Signal::new(None),
            repo_filter: Signal::new(None),
            sort: Signal::new(SortOrder::default()),
        }
    }
}

/// This window's browse state — the ambient one while a piece BUILDS, the FOCUSED window's when
/// a command runs later from a handler that belongs to no scope (docs/state.md).
fn scene() -> Scene {
    Scene::try_ambient()
        .or_else(Scene::focused)
        .expect("no window is open, so there is no Scene to act on")
}

/// The APP's state: one catalog, one sync, one install at a time. Every window reads the same
/// — a download in one window is the same download in the other.
#[derive(Clone, Copy)]
struct AppState {
    /// Bumped after every committed catalog write; every catalog snapshot refetches on it.
    catalog_version: Signal<u64>,
    sync: Signal<SyncUi>,
    install: Signal<Option<InstallUi>>,
    repos: Signal<Vec<RepoRow>>,
    categories: Signal<Vec<Category>>,
}

impl Ambient for AppState {
    fn create() -> Self {
        let catalog_version = Signal::new(0);
        let st = AppState {
            catalog_version,
            sync: Signal::new(SyncUi::Idle),
            install: Signal::new(None),
            repos: snapshot(
                Vec::new(),
                move || catalog_version.get(),
                |_| catalog::read(catalog::repos),
            ),
            categories: snapshot(
                Vec::new(),
                move || catalog_version.get(),
                |_| catalog::read(catalog::categories),
            ),
        };
        // Cross-thread write handles for the sync and install workers.
        let _ = SYNC_SETTER.set(st.sync.setter());
        let _ = INSTALL_SETTER.set(st.install.setter());
        let _ = CATALOG_SETTER.set(st.catalog_version.setter());
        st
    }
}

// Cross-thread write handles, populated at init on the main thread.
static SYNC_SETTER: OnceLock<Setter<SyncUi>> = OnceLock::new();
static INSTALL_SETTER: OnceLock<Setter<Option<InstallUi>>> = OnceLock::new();
static CATALOG_SETTER: OnceLock<Setter<u64>> = OnceLock::new();
/// The app currently downloading/installing, so the install callback can label its result and
/// clean up the staged APK once the installer is done with it.
#[derive(Clone)]
struct InFlight {
    pkg: String,
    name: String,
    /// The catalog this app is being installed from, recorded on success so App Fair knows it owns
    /// the install and which source it came from (#9).
    repo_id: u64,
    /// The downloaded APK on disk, deleted once the install reaches a terminal state.
    apk_path: PathBuf,
}
static IN_FLIGHT: Mutex<Option<InFlight>> = Mutex::new(None);

pub fn query() -> Signal<String> {
    scene().query
}
pub fn category() -> Signal<Option<String>> {
    scene().category
}
pub fn repo_filter() -> Signal<Option<u64>> {
    scene().repo_filter
}
pub fn sort_order() -> Signal<SortOrder> {
    scene().sort
}
pub fn catalog_version() -> Signal<u64> {
    AppState::app().catalog_version
}
pub fn sync_signal() -> Signal<SyncUi> {
    AppState::app().sync
}
pub fn install_signal() -> Signal<Option<InstallUi>> {
    AppState::app().install
}

/// Whether a download/install is mid-flight (so an auto-update batch shouldn't barge in).
pub fn install_active() -> bool {
    install_signal().get_untracked().is_some_and(|u| {
        matches!(
            u.phase,
            InstallPhase::Authorizing | InstallPhase::Downloading | InstallPhase::Installing
        )
    })
}

// --- the catalog (owned by its worker) --------------------------------------------------------

/// A signal holding what `fetch` last produced for `source`'s current value. The fetch runs
/// again whenever the source changes (every source here reads [`catalog_version`], so every
/// committed catalog write refreshes it), and the previous value stays on screen while the
/// next one loads. A superseded fetch never lands.
pub fn snapshot<S, T, F>(
    initial: T,
    source: impl Fn() -> S + 'static,
    fetch: impl Fn(S) -> F + 'static,
) -> Signal<T>
where
    S: Clone + PartialEq + 'static,
    T: 'static,
    F: Future<Output = Result<T, DbError>> + 'static,
{
    let value = Signal::new(initial);
    Resource::new(source, move |s| {
        let fetched = fetch(s);
        async move {
            match fetched.await {
                Ok(v) => value.set(v),
                Err(e) => warn!("catalog read failed: {e}"),
            }
            Ok::<(), Infallible>(())
        }
    });
    value
}

/// Run `f` as one catalog transaction, then refresh every page that reads the catalog.
pub fn write(f: impl FnOnce(&ModelContainer) -> Result<(), DbError> + Send + 'static) {
    let committed = catalog::write(f);
    day::task(async move {
        match committed.await {
            Ok(()) => bump_catalog(),
            Err(e) => error!("catalog write failed: {e}"),
        }
    });
}

/// Every catalog, for the catalog switcher and Settings.
pub fn repos() -> Signal<Vec<RepoRow>> {
    AppState::app().repos
}

/// The enabled catalogs' categories, for the category filter.
pub fn categories() -> Signal<Vec<Category>> {
    AppState::app().categories
}

/// The apps this window's browse state selects.
pub fn search_results() -> Signal<Vec<AppSummary>> {
    let scene = scene();
    let version = catalog_version();
    snapshot(
        Vec::new(),
        move || {
            let search = catalog::Search {
                query: scene.query.get(),
                category: scene.category.get(),
                repo: scene.repo_filter.get(),
                exclude_anti: crate::settings::excluded_anti_features(),
                sort: scene.sort.get(),
                // The device locale drives the "Name" collation; cached, so this is cheap.
                locale: platform::device_locale(),
            };
            (search, version.get())
        },
        |(search, _)| {
            let found = catalog::read(move |db| catalog::search(db, &search));
            async move {
                let found = found.await?;
                remember_names(&found);
                Ok(found)
            }
        },
    )
}

/// Installed user apps that are also in the catalog, each with its on-device versionCode. The
/// slow package enumeration is cached (see [`cached_installed_packages`]); the catalog lookups
/// run again on every catalog change, so a sync's newer versions show up.
pub fn installed_updates() -> Signal<Vec<(AppSummary, i64)>> {
    let version = catalog_version();
    if crate::market::enabled() {
        // iOS: the apps this marketplace installed, by Apple item id, matched to their listings.
        let library = crate::market::library();
        return snapshot(
            Vec::new(),
            move || (version.get(), library.get().generation),
            move |_| {
                let installed: Vec<(u64, i64)> = library
                    .get_untracked()
                    .apps
                    .values()
                    .filter_map(|a| {
                        Some((a.id, crate::market::installed_code(a.installed.as_ref()?)))
                    })
                    .collect();
                let found = catalog::read(move |db| {
                    let items: Vec<u64> = installed.iter().map(|(i, _)| *i).collect();
                    let pkgs = catalog::packages_for_items(db, &items)?;
                    let mut out = Vec::new();
                    for (item, code) in installed {
                        if let Some(pkg) = pkgs.get(&item)
                            && let Some(s) = catalog::summary_by_pkg(db, pkg)?
                        {
                            out.push((s, code));
                        }
                    }
                    Ok(out)
                });
                async move {
                    let found = found.await?;
                    remember_names(found.iter().map(|(s, _)| s));
                    Ok(found)
                }
            },
        );
    }
    snapshot(
        Vec::new(),
        move || version.get(),
        |_| {
            let installed = cached_installed_packages();
            let found = catalog::read(move |db| {
                let mut out = Vec::new();
                for (pkg, code) in installed {
                    if let Some(s) = catalog::summary_by_pkg(db, &pkg)? {
                        out.push((s, code));
                    }
                }
                Ok(out)
            });
            async move {
                let found = found.await?;
                remember_names(found.iter().map(|(s, _)| s));
                Ok(found)
            }
        },
    )
}

/// The display names of the apps the lists have shown, by route key: what a pushed detail
/// page's nav bar reads, synchronously, for its title.
#[derive(Clone, Default)]
struct Names(std::rc::Rc<RefCell<HashMap<String, String>>>);

impl Ambient for Names {
    fn create() -> Self {
        Names::default()
    }
}

fn remember_names<'a>(apps: impl IntoIterator<Item = &'a AppSummary>) {
    let names = Names::app();
    let mut names = names.0.borrow_mut();
    for a in apps {
        names.insert(catalog::app_key(a.repo_id, &a.pkg), a.name.clone());
    }
}

/// The name a list showed for this app, if one did.
pub fn app_name(repo_id: u64, pkg: &str) -> Option<String> {
    Names::app()
        .0
        .borrow()
        .get(&catalog::app_key(repo_id, pkg))
        .cloned()
}

/// The installed-package list tagged with the install generation it was enumerated at.
type InstalledPkgsCache = Option<(u64, Vec<(String, i64)>)>;

/// Cached `platform::installed_packages()` result — the slow JNI enumeration. App-wide: what
/// the device has installed is a fact about the device.
#[derive(Clone, Default)]
struct InstalledPkgs(std::rc::Rc<RefCell<InstalledPkgsCache>>);

impl Ambient for InstalledPkgs {
    fn create() -> Self {
        InstalledPkgs::default()
    }
}

/// Bumps only when the set of installed packages can have changed (an install or uninstall). The
/// package enumeration is cached against it so frequent *catalog* bumps (every sync) don't re-run
/// the JNI call.
static INSTALL_GEN: AtomicU64 = AtomicU64::new(0);

/// Invalidate the installed-package cache after an install/uninstall so the next read re-enumerates.
pub fn bump_installed() {
    INSTALL_GEN.fetch_add(1, Ordering::Relaxed);
}

/// The on-device installed packages (pkg, versionCode), enumerated over JNI on the main thread.
/// Cached against [`INSTALL_GEN`]: this JNI call is slow, and only an actual install or
/// uninstall invalidates it.
fn cached_installed_packages() -> Vec<(String, i64)> {
    let generation = INSTALL_GEN.load(Ordering::Relaxed);
    let cache = InstalledPkgs::app();
    if let Some((_, v)) = cache.0.borrow().as_ref().filter(|(g, _)| *g == generation) {
        return v.clone();
    }
    let installed = platform::installed_packages();
    *cache.0.borrow_mut() = Some((generation, installed.clone()));
    installed
}

// --- cross-thread pushes -----------------------------------------------------

pub fn set_sync(ui: SyncUi) {
    if let Some(s) = SYNC_SETTER.get() {
        s.set(ui);
    }
}

pub fn set_install(ui: Option<InstallUi>) {
    if let Some(s) = INSTALL_SETTER.get() {
        s.set(ui);
    }
}

/// Bump the catalog version so every list/category binding re-queries the DB.
pub fn bump_catalog() {
    if let Some(s) = CATALOG_SETTER.get() {
        // Read-modify-write off the main thread is unsafe on a Signal; instead push a fresh,
        // monotonically increasing token. Any change is enough to invalidate the bindings.
        s.set(next_token());
    }
}

fn next_token() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Install a batch of updates one at a time (#8): the first starts now, and each subsequent one
/// starts when the previous reaches a terminal state. A no-op if `apps` is empty.
pub fn start_updates(mut apps: Vec<AppDetail>) {
    if apps.is_empty() {
        return;
    }
    let first = apps.remove(0);
    *UPDATE_QUEUE.lock().unwrap() = apps;
    crate::install::start(&first);
}

/// Start the next queued update, if any. Returns whether one was started.
fn start_next_update() -> bool {
    let next = {
        let mut q = UPDATE_QUEUE.lock().unwrap();
        if q.is_empty() {
            None
        } else {
            Some(q.remove(0))
        }
    };
    match next {
        Some(app) => {
            crate::install::start(&app);
            true
        }
        None => false,
    }
}

/// Record the app being installed and the APK staged for it (for result labelling + cleanup).
pub fn set_in_flight(pkg: &str, name: &str, repo_id: u64, apk_path: &Path) {
    *IN_FLIGHT.lock().unwrap() = Some(InFlight {
        pkg: pkg.to_string(),
        name: name.to_string(),
        repo_id,
        apk_path: apk_path.to_path_buf(),
    });
}

fn in_flight() -> Option<InFlight> {
    IN_FLIGHT.lock().unwrap().clone()
}

/// Called from the Android install-result JNI callback (`platform`): maps a PackageInstaller
/// status into the install sheet's final state, and deletes the staged APK once the installer is
/// done with it.
pub fn on_native_install_status(status: i32, message: Option<String>) {
    let record = in_flight();
    let pkg = record.as_ref().map(|r| r.pkg.clone()).unwrap_or_default();
    let name = record.as_ref().map(|r| r.name.clone()).unwrap_or_default();
    let repo_id = record.as_ref().map(|r| r.repo_id).unwrap_or_default();
    // PackageInstaller status codes.
    const STATUS_SUCCESS: i32 = 0;
    const STATUS_PENDING_USER_ACTION: i32 = -1;
    const STATUS_FAILURE_ABORTED: i32 = 3;

    let phase = match status {
        // The system confirm screen is up; keep the sheet in its Installing state and hold the
        // in-flight record for the eventual terminal status.
        STATUS_PENDING_USER_ACTION => InstallPhase::Installing,
        STATUS_SUCCESS => InstallPhase::Success,
        STATUS_FAILURE_ABORTED => InstallPhase::Cancelled,
        _ => InstallPhase::Failed(message.unwrap_or_else(|| "Install failed".into())),
    };

    // On any terminal status the installer has already copied the APK into its own session, so the
    // staged file on disk is no longer needed — delete it (a large APK must not linger) and clear
    // the record. `Installing` (PENDING) keeps both for the result that follows.
    if !matches!(phase, InstallPhase::Installing) {
        if let Some(r) = &record {
            let _ = std::fs::remove_file(&r.apk_path);
        }
        *IN_FLIGHT.lock().unwrap() = None;
    }

    if phase == InstallPhase::Success {
        // Remember App Fair installed this app and from which catalog, so the detail page can offer
        // to open or uninstall it and name its source (#9).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // The installed set changed — invalidate the package cache so Updates/detail re-read it.
        bump_installed();
        if pkg.is_empty() {
            bump_catalog();
        } else {
            let pkg = pkg.clone();
            write(move |db| catalog::record_install(db, &pkg, repo_id, now));
        }
    }
    set_install(Some(InstallUi {
        pkg,
        name,
        phase: phase.clone(),
        progress: 1.0,
        downloaded: 0,
        total: 0,
    }));

    // Drive a "Update all" batch (#8): advance to the next update after a finished one; a user
    // cancel abandons the rest of the batch. `Installing` (system confirm still up) is not terminal.
    match phase {
        InstallPhase::Success | InstallPhase::Failed(_) => {
            start_next_update();
        }
        InstallPhase::Cancelled => {
            UPDATE_QUEUE.lock().unwrap().clear();
        }
        InstallPhase::Authorizing | InstallPhase::Downloading | InstallPhase::Installing => {}
    }
}
