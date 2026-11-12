//! App Fair — an open app store built with [Day](https://daybrite.dev). It syncs App Index,
//! AltStore, and F-Droid Index V2 catalogs into an on-device store and lets you browse, search,
//! and switch between them. On Android it downloads, installs, updates, and launches APKs through
//! the system package installer; on iOS it is an alternative app marketplace that installs,
//! updates, opens, and deletes apps through Apple's MarketplaceKit (`market`).
//!
//! `root()` is a three-tab shell (Catalogs, Updates, Settings). Catalogs and Updates are each a
//! navigation stack that pushes an app detail page; an install overlay floats above everything.

use day::prelude::*;
use day_piece_settings::prelude::Prefs;

mod altstore;
mod appindex;
mod cache;
mod catalog;
mod catalogs_add;
mod collate;
mod detail;
mod fdroid;
mod icons;
mod install;
mod install_sheet;
mod market;
mod mock;
mod model;
mod net;
mod platform;
mod settings;
mod state;
mod sync;
mod ui;
mod updates;
mod util;

// Typed constants for everything under resource/: https://daybrite.dev/docs/how-to/resources
day::resources!();
// The mobile entry points; desktop enters through src/main.rs.
day::day_start!(options: res::window_options(), root);

day::routes! {
    /// The three top-level tabs.
    pub(crate) enum Tab {
        Catalogs => "catalogs",
        Updates => "updates",
        Settings => "settings",
    }
}

/// Destinations pushed onto a tab's navigation stack.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Nav {
    /// An app detail page (repo id + package id encode into the route key).
    App { repo_id: u64, pkg: String },
    /// The "add a catalog" form.
    AddCatalog,
}

impl Route for Nav {
    fn key(&self) -> String {
        match self {
            Nav::App { repo_id, pkg } => format!("app-{}", catalog::app_key(*repo_id, pkg)),
            Nav::AddCatalog => "add-catalog".to_string(),
        }
    }
    fn from_key(key: &str) -> Option<Self> {
        if key == "add-catalog" {
            return Some(Nav::AddCatalog);
        }
        let (repo_id, pkg) = catalog::parse_app_key(key.strip_prefix("app-")?)?;
        Some(Nav::App {
            repo_id,
            pkg: pkg.to_string(),
        })
    }
    /// The native nav-bar title: the app's name as the list showed it, not the wire key.
    fn title(&self) -> String {
        match self {
            Nav::App { repo_id, pkg } => {
                state::app_name(*repo_id, pkg).unwrap_or_else(|| pkg.clone())
            }
            Nav::AddCatalog => res::str::nav_add_catalog().format(),
        }
    }
}

/// Build the destination page for a pushed [`Nav`].
pub(crate) fn nav_destination(n: &Nav) -> AnyPiece {
    match n {
        Nav::App { repo_id, pkg } => detail::detail_page(*repo_id, pkg.clone()).any(),
        Nav::AddCatalog => catalogs_add::add_catalog_page().any(),
    }
}

/// Push a destination onto a stack path.
pub(crate) fn push_nav(path: Signal<Vec<Nav>>, dest: Nav) {
    let mut v = path.get_untracked();
    v.push(dest);
    path.set(v);
}

pub fn root() -> impl Piece {
    // Resolve the device facts on the UI thread: they go through the app-class JNI bridge, which
    // only resolves here, and the sync threads read the cached values. The locale drives a
    // catalog re-import on change (sync::run); the SDK and ABIs pick the compatible version at
    // sync time.
    let _ = platform::device_locale();
    let _ = platform::device_sdk();
    let _ = platform::device_abis();
    // Open the catalog off the UI thread while the first page builds; its first read finds the
    // worker ready.
    std::thread::spawn(|| {
        catalog::worker();
    });
    // Delete any staged APK left by a previous session (interrupted download or an install whose
    // result never arrived). Safe now — no install is in flight.
    std::thread::spawn(install::prune_stale_apks);
    // iOS: follow the MarketplaceKit library (installs, updates, deletions) from the start.
    market::start();
    if mock::enabled() {
        // Offline, deterministic mode (local testing + CI screenshots): seed the bundled catalog
        // instead of syncing from the network, and skip background update checks.
        day::reactive::on_main(mock::seed);
    } else {
        // Load the image loader's mirror map from the stored catalog BEFORE the list asks for
        // icons. A metadata-only repo (appfair.net) hosts no images itself and points at
        // F-Droid mirrors; without the map, icons requested on launch 404 on the primary.
        // The worker answers in order, so the map is in place before the list's first read.
        sync::refresh_mirrors();
        // Check every enabled catalog for updates after first paint, unless Settings says not to.
        day::reactive::on_main(sync::sync_at_launch);
        // Register the daily background update check (idempotent; keeps any existing schedule) (#7).
        platform::schedule_update_checks();
    }

    // Settings and About, declared once for every window.
    let prefs = settings::preferences();

    // File ▸ New Window (docs/windows.md) — on mobile this is a second scene: an iPad window or
    // an Android document activity. The SAME shell builds again, which is why the browse state
    // (search, filters, sort) and the tab stacks live on a `Scene` rather than in globals.
    let again = prefs.clone();
    day::register_new_window(move || window_shell(again.clone()));

    window_shell(prefs)
}

/// One window's UI — the first window's, and every File ▸ New Window's.
fn window_shell(prefs: Prefs) -> impl Piece {
    state::Scene::scoped(move |_scene| {
        let tab = Signal::new(Tab::Catalogs);
        let catalog_path = Signal::new(Vec::<Nav>::new());
        let updates_path = Signal::new(Vec::<Nav>::new());

        let tabs = nav(tab)
            .style(NavStyle::Tabs)
            .items((
                item(Tab::Catalogs, res::str::tab_catalogs())
                    .icon(res::vectors::tab_catalogs)
                    .page(move || {
                        nav_stack(catalog_path, ui::catalogs_page(catalog_path))
                            .title(res::str::app_title())
                            .destination(nav_destination)
                    }),
                item(Tab::Updates, res::str::tab_updates())
                    .icon(res::vectors::tab_updates)
                    .page(move || {
                        nav_stack(updates_path, updates::updates_page(updates_path))
                            .title(res::str::nav_updates())
                            .destination(nav_destination)
                    }),
                // App Fair runs on the phones, where Settings is always a tab.
                prefs
                    .nav_item_everywhere(Tab::Settings)
                    .icon(res::vectors::tab_settings),
            ))
            .id(day::id!(tabs));

        // The install overlay floats above everything whenever an install is in progress.
        let install = state::install_signal();
        zstack((
            tabs,
            when(
                move || install.get().is_some(),
                install_sheet::install_sheet,
            ),
        ))
    })
}
