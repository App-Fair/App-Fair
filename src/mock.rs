//! A bundled F-Droid catalog subset for offline, deterministic runs — local testing and CI
//! screenshot generation without downloading the full ~50 MB index. Gated by the `APP_FAIR_MOCK`
//! environment variable (the CI passes it through `launch-env`, and it reaches the app as a normal
//! env var). When it's set, `root()` calls [`seed`] to load the embedded `mock/catalog.json` into
//! the catalog and pre-warm the icon cache from `mock/icons/`, instead of syncing over the
//! network — so the walkthrough always shows the same tasteful set of apps.

use crate::fdroid::{self, IndexV2};
use crate::{catalog, icons, model, state};

/// The curated catalog (12 well-known, non-controversial F-Droid apps, real metadata), embedded at
/// build time from a verbatim subset of the F-Droid Index V2.
const CATALOG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/mock/catalog.json"));

/// Each app's real icon, embedded so the seeded catalog renders fully offline — `(pkg, PNG bytes)`.
const ICONS: &[(&str, &[u8])] = &[
    (
        "app.organicmaps",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/app.organicmaps.png"
        )),
    ),
    (
        "com.ichi2.anki",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/com.ichi2.anki.png"
        )),
    ),
    (
        "com.keylesspalace.tusky",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/com.keylesspalace.tusky.png"
        )),
    ),
    (
        "de.danoeh.antennapod",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/de.danoeh.antennapod.png"
        )),
    ),
    (
        "eu.faircode.email",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/eu.faircode.email.png"
        )),
    ),
    (
        "net.gsantner.markor",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/net.gsantner.markor.png"
        )),
    ),
    (
        "org.breezyweather",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/org.breezyweather.png"
        )),
    ),
    (
        "org.fdroid.fdroid",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/org.fdroid.fdroid.png"
        )),
    ),
    (
        "org.fossify.gallery",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/org.fossify.gallery.png"
        )),
    ),
    (
        "org.mozilla.fennec_fdroid",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/org.mozilla.fennec_fdroid.png"
        )),
    ),
    (
        "org.videolan.vlc",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/org.videolan.vlc.png"
        )),
    ),
    (
        "org.wikipedia",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/org.wikipedia.png"
        )),
    ),
];

/// The iOS build's curated catalog: App Fair's own apps as an App Index, listed for the
/// marketplace (real names, versions and Apple item ids from appfair.net's published index).
const IOS_CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/mock/appindex-ios.json"
));

/// The iOS catalog's icons, by bundle identifier.
const IOS_ICONS: &[(&str, &[u8])] = &[
    (
        "org.appfair.app.Faire-Games",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/org.appfair.app.Faire-Games.png"
        )),
    ),
    (
        "org.appfair.app.Net-Skip",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/org.appfair.app.Net-Skip.png"
        )),
    ),
    (
        "org.appfair.app.Stanza-Redux",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/mock/icons/org.appfair.app.Stanza-Redux.png"
        )),
    ),
];

/// Whether to run against the bundled catalog instead of the network — set by `APP_FAIR_MOCK`
/// (any non-empty value other than `0`).
pub fn enabled() -> bool {
    std::env::var("APP_FAIR_MOCK").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Seed the catalog and icon cache from the embedded catalog at launch, replacing any prior sync
/// for the default repo. The icons land on disk before the import commits, so the list's first
/// rows find them. Safe to call again.
pub fn seed() {
    if catalog::PLATFORM == "ios" {
        seed_ios();
        return;
    }
    let Ok(index) = serde_json::from_str::<IndexV2>(CATALOG) else {
        day::warn!("mock catalog failed to parse");
        return;
    };
    // Follow the UI language (set from `DAY_LOCALE`) so a French run shows French catalog fields
    // where F-Droid has them; `fdroid::pick` falls back to `en-US` otherwise.
    let locale = std::env::var("DAY_LOCALE").unwrap_or_else(|_| "en-US".to_string());
    let repo_addr = model::split_index_url(catalog::DEFAULT_INDEX_URL).0;
    preseed_icons(&index, &repo_addr, &locale);
    state::write(move |db| {
        let repo_id = catalog::ensure_repo(db, &repo_addr)?;
        catalog::upsert_index(db, repo_id, &index, &locale, true).map(|_| ())
    });
}

/// The iOS seed: the App Index catalog into the default iOS catalog (id 1), icons first.
fn seed_ios() {
    let Ok(doc) = serde_json::from_str::<crate::appindex::AppIndex>(IOS_CATALOG) else {
        day::warn!("mock iOS catalog failed to parse");
        return;
    };
    let locale = std::env::var("DAY_LOCALE").unwrap_or_else(|_| "en-US".to_string());
    for (bundle, bytes) in IOS_ICONS {
        let found = doc
            .apps
            .iter()
            .filter_map(|a| a.platforms.get("ios"))
            .find(|p| p.bundle_identifier == *bundle);
        if let Some(icon) = found.and_then(|p| p.assets.icon.as_ref()) {
            icons::preseed(&icon.location, bytes);
        }
    }
    state::write(move |db| {
        let repo_id = catalog::ensure_repo(db, catalog::DEFAULT_APPINDEX_URL)?;
        let import = crate::appindex::import(&doc, repo_id, &locale, "ios", 0);
        catalog::import(db, repo_id, import, &locale, true).map(|_| ())
    });
}

/// Write each app's embedded icon into the on-disk image cache under the exact URL the list will
/// request (`repo_address + icon_path`), so icons render without any network fetch. Icons are
/// locale-independent, but resolve the path the same way `upsert_index` did so the URLs match.
fn preseed_icons(index: &IndexV2, repo_addr: &str, locale: &str) {
    for (pkg, bytes) in ICONS {
        if let Some(package) = index.packages.get(*pkg)
            && let Some(icon) = fdroid::pick(&package.metadata.icon, locale)
        {
            icons::preseed(&format!("{repo_addr}{}", icon.name), bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_and_icons_stay_consistent() {
        let index: IndexV2 = serde_json::from_str(CATALOG).expect("mock catalog parses");
        assert!(
            index.packages.len() >= 10,
            "expected a dozen curated apps, got {}",
            index.packages.len()
        );
        // Every package ships a non-empty icon so the seeded list renders fully offline...
        let icon_pkgs: HashSet<&str> = ICONS.iter().map(|(p, _)| *p).collect();
        for pkg in index.packages.keys() {
            assert!(
                icon_pkgs.contains(pkg.as_str()),
                "no bundled icon for {pkg}"
            );
        }
        // ...and every bundled icon maps to a package (no orphans).
        for (pkg, bytes) in ICONS {
            assert!(
                index.packages.contains_key(*pkg),
                "icon for missing pkg {pkg}"
            );
            assert!(!bytes.is_empty(), "empty icon for {pkg}");
        }
    }

    #[test]
    fn mock_catalog_seeds_into_the_catalog() {
        let index: IndexV2 = serde_json::from_str(CATALOG).unwrap();
        let db = catalog::open(day_persistence::Sqlite::memory()).unwrap();
        let repo_addr = model::split_index_url(catalog::DEFAULT_INDEX_URL).0;
        let repo_id = catalog::ensure_repo(&db, &repo_addr).unwrap();
        let written = catalog::upsert_index(&db, repo_id, &index, "en-US", true).unwrap();
        assert_eq!(written, index.packages.len());
        // A known app round-trips through the real query path.
        let wiki = catalog::detail(&db, repo_id, "org.wikipedia")
            .unwrap()
            .unwrap();
        assert_eq!(wiki.name, "Wikipedia");
        assert!(!wiki.signer.is_empty(), "signer parsed from the manifest");
        // The walkthrough opens Organic Maps by its row key, and expects it in the default
        // catalog with id 1.
        assert_eq!(repo_id, 1);
        assert!(
            catalog::detail(&db, repo_id, "app.organicmaps")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn the_ios_catalog_lists_marketplace_apps_with_icons() {
        let doc: crate::appindex::AppIndex =
            serde_json::from_str(IOS_CATALOG).expect("mock iOS catalog parses");
        let import = crate::appindex::import(&doc, 1, "en-US", "ios", 0);
        assert_eq!(import.apps.len(), IOS_ICONS.len());
        for app in &import.apps {
            assert!(app.apple_item_id > 0, "{} has no Apple item id", app.pkg);
            assert!(
                IOS_ICONS
                    .iter()
                    .any(|(b, bytes)| *b == app.pkg && !bytes.is_empty()),
                "no bundled icon for {}",
                app.pkg
            );
        }
        // The walkthrough opens Net Skip by its row key, first in the catalog's rank.
        assert_eq!(
            import.apps.iter().find(|a| a.rank == 0).unwrap().pkg,
            "org.appfair.app.Net-Skip"
        );
    }
}
