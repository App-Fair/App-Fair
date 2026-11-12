// Copyright © The Daybrite Project
// SPDX-License-Identifier: MPL-2.0

//! App Fair's iOS installer: the Apple alternative app marketplace, through MarketplaceKit
//! (`app_fair_marketplace`). It keeps the same promises the Android installer keeps, through a
//! very different machine:
//!
//! | Android (`install`, `platform`)             | iOS (this module)                                   |
//! |---------------------------------------------|-----------------------------------------------------|
//! | App Fair's Install button downloads the APK | MarketplaceKit's `ActionButton`; the system downloads |
//! | SHA-256 and signer checked against the catalog | the marketplace server signs an install token; the system checks Apple's package signature |
//! | `PackageInstaller` result over JNI          | `AppLibrary` snapshots and button events over a bridge stream |
//! | download progress, Cancel                   | the installation's `Progress`, cancelled through it |
//! | Launch, Uninstall                           | `ActionButton` launch and delete actions            |
//! | installed versionCode                       | the installed `CFBundleVersion` and Apple version id |
//! | WorkManager update checks                   | the marketplace extension's automatic updates       |
//!
//! The install sheet, the provenance record, and the Updates tab are shared with Android: this
//! module feeds them the same `InstallUi` phases and `catalog::record_install` calls.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use app_fair_marketplace as mk;
use day::prelude::*;

use crate::catalog;
use crate::model::AppDetail;
use crate::state::{self, InstallPhase, InstallUi};

pub use mk::{InstalledVersion, Status};

/// Whether this build installs through the iOS marketplace.
pub fn enabled() -> bool {
    mk::support() != mk::Support::Unsupported
}

/// Where the marketplace lives: its API, the base its packages are published under, and the
/// domain packages must be on. `APP_FAIR_MARKETPLACE` points a development build at another API
/// (a `wrangler dev` Worker, say), whose `/packages/` path then serves the packages.
struct Server {
    api: String,
    packages: String,
    domain: String,
}

fn server() -> &'static Server {
    static SERVER: OnceLock<Server> = OnceLock::new();
    SERVER.get_or_init(|| {
        match std::env::var("APP_FAIR_MARKETPLACE")
            .ok()
            .filter(|s| s.starts_with("https://"))
        {
            Some(url) => {
                let api = url.trim_end_matches('/').to_string();
                let domain = api["https://".len()..]
                    .split(['/', ':'])
                    .next()
                    .unwrap_or_default()
                    .to_string();
                Server {
                    packages: format!("{api}/packages"),
                    api,
                    domain,
                }
            }
            None => Server {
                api: mk::DEFAULT_SERVER.to_string(),
                packages: mk::DEFAULT_PACKAGES.to_string(),
                domain: mk::DEFAULT_DOMAIN.to_string(),
            },
        }
    })
}

/// This device's anonymous account, once the keychain has answered.
static ACCOUNT: OnceLock<String> = OnceLock::new();

/// What the marketplace knows, as the pages read it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Library {
    /// The system's apps from this marketplace, by Apple item id.
    pub apps: HashMap<u64, mk::LibraryApp>,
    pub status: Option<Status>,
    /// Bumps on every change, so a page can key on it cheaply.
    pub generation: u64,
}

/// The library as the UI thread last saw it, behind a cross-thread setter like the install
/// sheet's.
#[derive(Clone, Copy)]
struct Market {
    library: Signal<Library>,
}

impl Ambient for Market {
    fn create() -> Self {
        let library = Signal::new(Library::default());
        let _ = LIBRARY_SETTER.set(library.setter());
        Market { library }
    }
}

static LIBRARY_SETTER: OnceLock<Setter<Library>> = OnceLock::new();
/// The newest library, owned off the UI thread by the event handler.
static LATEST: Mutex<Option<Library>> = Mutex::new(None);

/// The marketplace library, a tracked read.
pub fn library() -> Signal<Library> {
    Market::app().library
}

fn publish(update: impl FnOnce(&mut Library)) {
    let mut latest = LATEST.lock().unwrap();
    let lib = latest.get_or_insert_with(Library::default);
    update(lib);
    lib.generation += 1;
    if let Some(s) = LIBRARY_SETTER.get() {
        s.set(lib.clone());
    }
}

/// An app a button was built for, so an event naming its item id can label the install sheet and
/// record where it came from.
#[derive(Clone)]
struct Listed {
    pkg: String,
    name: String,
    repo_id: u64,
}

static LISTED: Mutex<Option<HashMap<u64, Listed>>> = Mutex::new(None);

fn remember(app: &AppDetail) {
    LISTED
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(
            app.apple_item_id,
            Listed {
                pkg: app.pkg.clone(),
                name: app.name.clone(),
                repo_id: app.repo_id,
            },
        );
}

fn listed(item: u64) -> Option<Listed> {
    LISTED.lock().unwrap().as_ref()?.get(&item).cloned()
}

/// The installs one tap started, followed through to their end.
#[derive(Default)]
struct InFlight {
    items: Vec<u64>,
    /// The ones the system has shown as installing, so their disappearance from the installing
    /// set means they finished.
    started: HashSet<u64>,
    done: HashSet<u64>,
    /// Stopped without a new version and without an error: the person cancelled.
    stopped: HashSet<u64>,
    /// Each app's installed version id when the tap came, so an update that stops on the old
    /// version reads as stopped rather than finished.
    prior: HashMap<u64, Option<u64>>,
    failed: Option<String>,
}

static IN_FLIGHT: Mutex<Option<InFlight>> = Mutex::new(None);

/// Connect to MarketplaceKit: configure the bridge, resolve the account, read the device's
/// marketplace status, and start following the library. Called once from `root()` on iOS.
pub fn start() {
    if !enabled() {
        return;
    }
    let _ = library();
    if let Err(e) = mk::configure(&mk::Config {
        server: server().api.clone(),
        app_group: mk::DEFAULT_APP_GROUP.to_string(),
    }) {
        warn!("marketplace: configure: {e:?}");
    }
    day::task(async {
        match mk::account().await {
            Ok(id) => {
                let _ = ACCOUNT.set(id);
                // The extension needs the account; hand it over with the catalog's apps.
                catalog_changed();
            }
            Err(e) => error!("marketplace: no account id: {e:?}"),
        }
    });
    day::task(async {
        match mk::status().await {
            Ok(status) => publish(|l| l.status = Some(status)),
            Err(e) => warn!("marketplace: status: {e:?}"),
        }
    });
    if let Err(e) = mk::observe(handle) {
        warn!("marketplace: observe: {e:?}");
    }
}

/// One event from the system, on whatever thread it arrives.
fn handle(event: mk::Event) {
    match event {
        mk::Event::Library { apps, .. } => {
            let before: HashSet<u64> = LATEST
                .lock()
                .unwrap()
                .as_ref()
                .map(|l| {
                    l.apps
                        .values()
                        .filter(|a| a.installed.is_some())
                        .map(|a| a.id)
                        .collect()
                })
                .unwrap_or_default();
            let now: HashSet<u64> = apps
                .iter()
                .filter(|a| a.installed.is_some())
                .map(|a| a.id)
                .collect();
            follow(&apps);
            publish(|l| l.apps = apps.into_iter().map(|a| (a.id, a)).collect());
            // An app deleted (from App Fair's Delete button or the Home Screen) is no longer
            // App Fair's to manage: forget the provenance record, as Android does on uninstall.
            let gone: Vec<u64> = before.difference(&now).copied().collect();
            if !gone.is_empty() && !before.is_empty() {
                forget(gone);
            }
        }
        mk::Event::Confirming { ids } => {
            let prior = LATEST
                .lock()
                .unwrap()
                .as_ref()
                .map(|l| {
                    ids.iter()
                        .map(|id| {
                            let installed = l.apps.get(id).and_then(|a| a.installed.as_ref());
                            (*id, installed.map(|v| v.apple_version_id))
                        })
                        .collect()
                })
                .unwrap_or_default();
            *IN_FLIGHT.lock().unwrap() = Some(InFlight {
                items: ids.clone(),
                prior,
                ..Default::default()
            });
            sheet(&ids, InstallPhase::Authorizing, 0.0, 0, 0);
        }
        mk::Event::Confirmed { ids } => {
            if !ids.is_empty() {
                sheet(&ids, InstallPhase::Installing, 0.0, 0, 0);
            }
        }
        mk::Event::Declined { ids, message } => {
            let mut guard = IN_FLIGHT.lock().unwrap();
            if let Some(f) = guard.as_mut() {
                f.items.retain(|i| !ids.contains(i));
                f.failed = message.clone();
                if f.items.is_empty() {
                    *guard = None;
                    drop(guard);
                    let phase = InstallPhase::Failed(message.unwrap_or_else(|| {
                        "The marketplace did not authorize this install.".to_string()
                    }));
                    sheet(&ids, phase, 1.0, 0, 0);
                }
            }
        }
    }
}

/// What one library snapshot means for the installs a tap started.
#[derive(Debug, PartialEq)]
enum Step {
    /// Nothing to show yet: the system has not started, or reports no progress.
    Waiting,
    /// `item` is downloading or installing.
    Progress {
        item: u64,
        fraction: f32,
        completed: u64,
        total: u64,
    },
    /// Every install ended: `phase` is Success, Cancelled, or Failed.
    Ended {
        items: Vec<u64>,
        phase: InstallPhase,
    },
}

/// Read `apps` against `flight`, recording in it which installs started, finished (returned, for
/// their provenance records) and stopped. Pure, so the state machine is tested on any host.
fn advance(flight: &mut InFlight, apps: &[mk::LibraryApp]) -> (Step, Vec<u64>) {
    let by_id: HashMap<u64, &mk::LibraryApp> = apps.iter().map(|a| (a.id, a)).collect();
    let mut failed: Option<String> = None;
    let mut current: Option<(u64, &mk::Progress)> = None;
    let mut finished = Vec::new();
    for &item in &flight.items {
        let settled = flight.done.contains(&item) || flight.stopped.contains(&item);
        let Some(app) = by_id.get(&item) else {
            // A first install that stopped leaves the library altogether.
            if flight.started.contains(&item) && !settled {
                flight.stopped.insert(item);
            }
            continue;
        };
        if app.installing || app.updating {
            flight.started.insert(item);
            if current.is_none()
                && let Some(p) = &app.progress
            {
                current = Some((item, p));
            }
        } else if flight.started.contains(&item) && !settled {
            let prior = flight.prior.get(&item).copied().flatten();
            let new_version = app
                .installed
                .as_ref()
                .is_some_and(|v| prior != Some(v.apple_version_id));
            if let Some(e) = &app.error {
                failed = Some(e.clone());
            } else if new_version {
                flight.done.insert(item);
                finished.push(item);
            } else {
                flight.stopped.insert(item);
            }
        }
    }
    let settled = flight.done.len() + flight.stopped.len();
    // A "cancelled" error is the person stopping it, not a failure.
    let cancelled = failed
        .as_deref()
        .is_some_and(|e| e.to_ascii_lowercase().contains("cancel"));
    let step = if let Some(e) = failed.filter(|_| !cancelled) {
        Step::Ended {
            items: flight.items.clone(),
            phase: InstallPhase::Failed(e),
        }
    } else if settled == flight.items.len() || cancelled {
        Step::Ended {
            items: flight.items.clone(),
            phase: if flight.done.is_empty() {
                InstallPhase::Cancelled
            } else {
                InstallPhase::Success
            },
        }
    } else if let Some((item, p)) = current {
        Step::Progress {
            item,
            fraction: p.fraction.clamp(0.0, 1.0) as f32,
            completed: p.completed.max(0) as u64,
            total: p.total.max(0) as u64,
        }
    } else {
        Step::Waiting
    };
    (step, finished)
}

/// Advance the install sheet from a library snapshot: progress while the system installs,
/// then success (with the provenance record), a cancellation, or the system's error.
fn follow(apps: &[mk::LibraryApp]) {
    let mut guard = IN_FLIGHT.lock().unwrap();
    let Some(flight) = guard.as_mut() else {
        return;
    };
    let (step, finished) = advance(flight, apps);
    if matches!(step, Step::Ended { .. }) {
        *guard = None;
    }
    drop(guard);
    for item in finished {
        if let Some(l) = listed(item) {
            record(l);
        }
    }
    match step {
        Step::Waiting => {}
        Step::Progress {
            item,
            fraction,
            completed,
            total,
        } => sheet(
            &[item],
            InstallPhase::Downloading,
            fraction,
            completed,
            total,
        ),
        Step::Ended { items, phase } => {
            state::bump_installed();
            sheet(&items, phase, 1.0, 0, 0);
        }
    }
}

/// Show `phase` for the first of `items` in the install sheet.
fn sheet(items: &[u64], phase: InstallPhase, progress: f32, downloaded: u64, total: u64) {
    let first = items.first().copied().unwrap_or_default();
    let listed = listed(first);
    let name = match (&listed, items.len()) {
        (Some(l), 1) => l.name.clone(),
        (Some(l), n) => format!("{} (+{})", l.name, n - 1),
        (None, _) => String::new(),
    };
    state::set_install(Some(InstallUi {
        pkg: listed.map(|l| l.pkg).unwrap_or_default(),
        name,
        phase,
        progress,
        downloaded,
        total,
    }));
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Remember that App Fair installed this app and from which catalog (#9).
fn record(l: Listed) {
    let at = now();
    day::reactive::on_main(move || {
        state::write(move |db| catalog::record_install(db, &l.pkg, l.repo_id, at));
    });
}

/// Forget the provenance records of apps that left the device.
fn forget(items: Vec<u64>) {
    day::reactive::on_main(move || {
        state::write(move |db| {
            for pkg in catalog::packages_for_items(db, &items)?.into_values() {
                catalog::remove_install(db, &pkg)?;
            }
            Ok(())
        });
    });
}

/// Cancel the installation the sheet shows, where the system allows it.
pub fn cancel() {
    let items = IN_FLIGHT
        .lock()
        .unwrap()
        .as_ref()
        .map(|f| f.items.clone())
        .unwrap_or_default();
    for item in items {
        let _ = mk::cancel(item);
    }
}

/// The installed version of `app`, when this marketplace installed it.
pub fn installed(lib: &Library, app: &AppDetail) -> Option<InstalledVersion> {
    lib.apps.get(&app.apple_item_id)?.installed.clone()
}

/// Whether `installed` is at least the version the catalog offers.
pub fn up_to_date(installed: &InstalledVersion, app: &AppDetail) -> bool {
    if installed.apple_version_id != 0 && installed.apple_version_id == app.apple_version_id {
        return true;
    }
    crate::appindex::leading_int(&installed.version) >= app.version_code
}

/// The build number the Updates tab compares, from an installed version.
pub fn installed_code(installed: &InstalledVersion) -> i64 {
    crate::appindex::leading_int(&installed.version)
}

/// What a button installs for `app`.
pub fn target(app: &AppDetail, update: bool) -> mk::Target {
    let s = server();
    mk::Target {
        apple_item_id: app.apple_item_id,
        apple_version_id: (app.apple_version_id != 0).then_some(app.apple_version_id),
        bundle_id: app.pkg.clone(),
        package_url: mk::package_url(
            &s.packages,
            &s.domain,
            Some(app.package_url.as_str()).filter(|u| !u.is_empty()),
            app.apple_item_id,
        ),
        share_url: Some(app.share_url.clone()).filter(|u| !u.is_empty()),
        update,
    }
}

/// The look of App Fair's action buttons: the accent pill the Android build's prominent button
/// is.
fn style(width: f64) -> mk::Style {
    mk::Style {
        tint: Some("#ffffff".into()),
        background: Some(format!("#{:06x}", crate::ui::ACCENT_HEX)),
        font_size: Some(15.0),
        corner_radius: Some(16.0),
        width: Some(width),
        height: Some(32.0),
        ..Default::default()
    }
}

fn button(action: mk::Action, label: String, width: f64) -> impl Piece + use<> {
    let spec = mk::ButtonSpec {
        action,
        label,
        server: server().api.clone(),
        account: ACCOUNT.get().cloned().unwrap_or_default(),
        style: style(width),
    };
    mk::action_button(&spec).frame(width, 32.0)
}

/// Install or update `app`.
pub fn install_button(app: &AppDetail, update: bool, label: String) -> impl Piece + use<> {
    remember(app);
    button(mk::Action::Install(target(app, update)), label, 96.0)
}

/// Update every app in `apps` under one confirmation: "Update all".
pub fn update_all_button(apps: &[AppDetail], label: String) -> impl Piece + use<> {
    for app in apps {
        remember(app);
    }
    let targets = apps.iter().map(|a| target(a, true)).collect();
    button(mk::Action::Batch(targets), label, 120.0)
}

/// Open the installed `app`.
pub fn launch_button(app: &AppDetail, label: String) -> impl Piece + use<> {
    button(mk::Action::Launch(app.apple_item_id), label, 96.0)
}

/// Delete the installed `app`, after the system's own confirmation.
pub fn delete_button(app: &AppDetail, label: String) -> impl Piece + use<> {
    button(mk::Action::Delete(app.apple_item_id), label, 96.0)
}

/// Whether the device meets what `app` needs, by the installer's own rules.
pub async fn satisfies(app: &AppDetail) -> bool {
    let requirements = mk::Requirements {
        minimum_system_version: Some(app.min_os.clone()).filter(|s| !s.is_empty()),
        required_device_capabilities: Some(app.device_caps.clone()).filter(|c| !c.is_empty()),
        age_rating_rank: (app.age_rank > 0).then_some(app.age_rank),
        expected_install_size: (app.apk_size > 0).then_some(app.apk_size as u64),
    };
    mk::satisfies(&requirements).await.unwrap_or(true)
}

/// Hand the marketplace extension the account, the server, and the newest version the enabled
/// catalogs offer for each app, so the system's automatic updates can run while App Fair does
/// not. Called after every catalog import, from the sync thread.
pub fn catalog_changed() {
    if !enabled() {
        return;
    }
    let Some(account) = ACCOUNT.get().cloned() else {
        return;
    };
    std::thread::spawn(move || {
        let apps = match catalog::block_on(catalog::read(catalog::marketplace_apps)) {
            Ok(apps) => apps,
            Err(e) => {
                warn!("marketplace: reading the catalog for the extension: {e}");
                return;
            }
        };
        let shared = mk::Shared {
            server: server().api.clone(),
            account,
            apps: apps
                .iter()
                .map(|a| {
                    let t = target(a, true);
                    mk::SharedApp {
                        apple_item_id: a.apple_item_id,
                        apple_version_id: t.apple_version_id,
                        bundle_id: a.pkg.clone(),
                        package_url: t.package_url,
                        bundle_version: a.bundle_version.clone(),
                        short_version: a.version_name.clone(),
                        share_url: t.share_url,
                    }
                })
                .collect(),
        };
        if let Err(e) = mk::share(&shared) {
            // Expected in an unsigned or simulator build, which has no App Group container.
            debug!("marketplace: sharing with the extension: {e:?}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(
        id: u64,
        installed: Option<u64>,
        installing: bool,
        error: Option<&str>,
    ) -> mk::LibraryApp {
        mk::LibraryApp {
            id,
            installed: installed.map(|v| mk::InstalledVersion {
                version: "2".into(),
                short_version: "2.0".into(),
                apple_version_id: v,
                account: None,
            }),
            installing,
            updating: false,
            progress: installing.then_some(mk::Progress {
                fraction: 0.25,
                completed: 25,
                total: 100,
                cancellable: true,
            }),
            error: error.map(String::from),
        }
    }

    fn flight(items: &[u64], prior: &[(u64, Option<u64>)]) -> InFlight {
        InFlight {
            items: items.to_vec(),
            prior: prior.iter().copied().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn an_install_reports_progress_then_success() {
        let mut f = flight(&[1], &[(1, None)]);
        // Before the system starts, there is nothing to show.
        assert_eq!(advance(&mut f, &[]), (Step::Waiting, vec![]));
        let (step, done) = advance(&mut f, &[app(1, None, true, None)]);
        assert_eq!(
            step,
            Step::Progress {
                item: 1,
                fraction: 0.25,
                completed: 25,
                total: 100
            }
        );
        assert!(done.is_empty());
        let (step, done) = advance(&mut f, &[app(1, Some(10), false, None)]);
        assert_eq!(
            step,
            Step::Ended {
                items: vec![1],
                phase: InstallPhase::Success
            }
        );
        assert_eq!(done, vec![1]);
    }

    #[test]
    fn a_stopped_first_install_is_cancelled() {
        let mut f = flight(&[1], &[(1, None)]);
        advance(&mut f, &[app(1, None, true, None)]);
        // Cancelled mid-download: the app leaves the library.
        let (step, done) = advance(&mut f, &[]);
        assert_eq!(
            step,
            Step::Ended {
                items: vec![1],
                phase: InstallPhase::Cancelled
            }
        );
        assert!(done.is_empty());
    }

    #[test]
    fn an_update_that_ends_on_the_old_version_is_cancelled() {
        let mut f = flight(&[1], &[(1, Some(10))]);
        advance(&mut f, &[app(1, Some(10), true, None)]);
        let (step, _) = advance(&mut f, &[app(1, Some(10), false, None)]);
        assert_eq!(
            step,
            Step::Ended {
                items: vec![1],
                phase: InstallPhase::Cancelled
            }
        );
        // ...and one that ends on a new version succeeded.
        let mut f = flight(&[1], &[(1, Some(10))]);
        advance(&mut f, &[app(1, Some(10), true, None)]);
        let (step, done) = advance(&mut f, &[app(1, Some(11), false, None)]);
        assert!(matches!(
            step,
            Step::Ended {
                phase: InstallPhase::Success,
                ..
            }
        ));
        assert_eq!(done, vec![1]);
    }

    #[test]
    fn the_systems_error_fails_the_install_unless_it_is_a_cancellation() {
        let mut f = flight(&[1], &[(1, None)]);
        advance(&mut f, &[app(1, None, true, None)]);
        let (step, _) = advance(&mut f, &[app(1, None, false, Some("networkError"))]);
        assert_eq!(
            step,
            Step::Ended {
                items: vec![1],
                phase: InstallPhase::Failed("networkError".into())
            }
        );
        let mut f = flight(&[1], &[(1, None)]);
        advance(&mut f, &[app(1, None, true, None)]);
        let (step, _) = advance(&mut f, &[app(1, None, false, Some("cancelled"))]);
        assert!(matches!(
            step,
            Step::Ended {
                phase: InstallPhase::Cancelled,
                ..
            }
        ));
    }

    #[test]
    fn a_batch_ends_when_every_update_has() {
        let mut f = flight(&[1, 2], &[(1, Some(10)), (2, Some(20))]);
        advance(
            &mut f,
            &[app(1, Some(10), true, None), app(2, Some(20), true, None)],
        );
        // The first finishes; the second still installs and its progress shows.
        let (step, done) = advance(
            &mut f,
            &[app(1, Some(11), false, None), app(2, Some(20), true, None)],
        );
        assert!(matches!(step, Step::Progress { item: 2, .. }));
        assert_eq!(done, vec![1]);
        // A finished install is recorded once, however many snapshots follow.
        let (step, done) = advance(
            &mut f,
            &[app(1, Some(11), false, None), app(2, Some(21), false, None)],
        );
        assert_eq!(
            step,
            Step::Ended {
                items: vec![1, 2],
                phase: InstallPhase::Success
            }
        );
        assert_eq!(done, vec![2]);
    }

    #[test]
    fn versions_compare_by_apple_version_id_then_build() {
        let detail = AppDetail {
            version_code: 236,
            apple_version_id: 869899373,
            ..Default::default()
        };
        let v = |build: &str, id: u64| InstalledVersion {
            version: build.into(),
            short_version: String::new(),
            apple_version_id: id,
            account: None,
        };
        assert!(up_to_date(&v("1", 869899373), &detail));
        assert!(up_to_date(&v("236", 1), &detail));
        assert!(!up_to_date(&v("235", 1), &detail));
    }
}
