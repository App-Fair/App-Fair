// Copyright © The Daybrite Project
// SPDX-License-Identifier: MPL-2.0

//! App Index catalogs (https://appfair.org/schemas/appindex/): App Fair's own format, and the
//! primary one. One document describes any number of apps across platforms; App Fair imports, for
//! each app, the platform this device runs (`android` or `ios`), so one catalog serves both
//! stores.
//!
//! The base schema describes an app for its landing page. A store also needs to know where the
//! installable artifact is and how to check it, so App Fair reads the store fields the schema
//! defines alongside it (all optional, so a landing-page document still parses):
//!
//! - top level `catalog`: the catalog's name, mirrors, category and anti-feature labels, and an
//!   editorial `rank`;
//! - app level `categories`, `antiFeatures` and `developer`;
//! - platform level `released`, `minimumSystemVersion`, and `distribution`, with an `apk` (URL,
//!   size, digest, signing certificate, version code, SDK range, native code) for Android and an
//!   `adp` (Apple item and version ids, the alternative distribution package's URL, device
//!   capabilities, age rating) for an iOS alternative marketplace.

// The deserialized document carries more than the store shows (keywords, links, SBOMs); keep the
// shape so features can grow without re-deriving the parser.
#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::catalog::{App, Import, app_key};
use crate::fdroid::{Localized, pick};

/// The schema URI prefix every App Index document's `$schema` starts with.
pub const SCHEMA_PREFIX: &str = "https://appfair.org/schemas/appindex/";

#[derive(Debug, Deserialize)]
pub struct AppIndex {
    #[serde(rename = "$schema", default)]
    pub schema: String,
    #[serde(default, rename = "specVersion")]
    pub spec_version: String,
    /// ISO 8601; the import's change stamp when present.
    #[serde(default)]
    pub generated: String,
    #[serde(default)]
    pub catalog: Option<CatalogInfo>,
    #[serde(default)]
    pub apps: Vec<AppEntry>,
}

/// The store's description of the catalog itself.
#[derive(Debug, Deserialize, Default)]
pub struct CatalogInfo {
    #[serde(default)]
    pub name: Localized<String>,
    #[serde(default)]
    pub description: Localized<String>,
    #[serde(default)]
    pub homepage: String,
    /// Base URLs that serve the same document and assets, tried when the primary fails.
    #[serde(default)]
    pub mirrors: Vec<String>,
    #[serde(default)]
    pub categories: BTreeMap<String, Label>,
    #[serde(default, rename = "antiFeatures")]
    pub anti_features: BTreeMap<String, Label>,
    /// App `name`s in editorial order, highest first.
    #[serde(default)]
    pub rank: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Label {
    #[serde(default)]
    pub name: Localized<String>,
    #[serde(default)]
    pub description: Localized<String>,
}

#[derive(Debug, Deserialize)]
pub struct AppEntry {
    pub name: String,
    #[serde(default)]
    pub title: Localized<String>,
    #[serde(default)]
    pub subtitle: Localized<String>,
    #[serde(default)]
    pub description: Localized<String>,
    #[serde(default, rename = "releaseNotes")]
    pub release_notes: Localized<String>,
    #[serde(default)]
    pub platforms: BTreeMap<String, Platform>,
    #[serde(default)]
    pub links: BTreeMap<String, Localized<String>>,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default, rename = "antiFeatures")]
    pub anti_features: Vec<String>,
    #[serde(default)]
    pub developer: Developer,
}

#[derive(Debug, Deserialize, Default)]
pub struct Source {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub release: String,
    /// The base `assets[*].location` paths resolve against.
    #[serde(default)]
    pub assets: String,
    #[serde(default)]
    pub license: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct Developer {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct Platform {
    #[serde(default)]
    pub version: String,
    #[serde(default, rename = "buildNumber")]
    pub build_number: String,
    #[serde(default, rename = "bundleIdentifier")]
    pub bundle_identifier: String,
    #[serde(default, rename = "applicationId")]
    pub application_id: String,
    #[serde(default)]
    pub title: Localized<String>,
    #[serde(default)]
    pub subtitle: Localized<String>,
    #[serde(default)]
    pub description: Localized<String>,
    #[serde(default, rename = "releaseNotes")]
    pub release_notes: Localized<String>,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[serde(default)]
    pub assets: Assets,
    /// ISO 8601: when this version was published.
    #[serde(default)]
    pub released: String,
    #[serde(default, rename = "minimumSystemVersion")]
    pub minimum_system_version: String,
    #[serde(default)]
    pub distribution: Distribution,
}

#[derive(Debug, Deserialize)]
pub struct Permission {
    pub key: String,
    #[serde(default)]
    pub description: Localized<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Assets {
    #[serde(default)]
    pub icon: Option<ImageRef>,
    #[serde(default)]
    pub screenshots: Localized<Vec<ImageRef>>,
}

#[derive(Debug, Deserialize)]
pub struct ImageRef {
    pub location: String,
}

/// How a store installs this platform's build.
#[derive(Debug, Deserialize, Default)]
pub struct Distribution {
    #[serde(default)]
    pub apk: Option<Apk>,
    #[serde(default)]
    pub adp: Option<Adp>,
}

/// An Android package.
#[derive(Debug, Deserialize, Default)]
pub struct Apk {
    /// Absolute, or relative to `source.release`.
    pub url: String,
    #[serde(default)]
    pub size: i64,
    /// `sha256:<hex>` of the file.
    #[serde(default)]
    pub digest: String,
    /// `sha256:<hex>` of the signing certificate.
    #[serde(default)]
    pub signer: String,
    #[serde(default, rename = "versionCode")]
    pub version_code: i64,
    #[serde(default, rename = "minSdk")]
    pub min_sdk: i64,
    #[serde(default, rename = "targetSdk")]
    pub target_sdk: i64,
    #[serde(default, rename = "maxSdk")]
    pub max_sdk: i64,
    #[serde(default)]
    pub nativecode: Vec<String>,
}

/// An Apple alternative distribution package, installed through an alternative app marketplace.
#[derive(Debug, Deserialize, Default)]
pub struct Adp {
    /// The app's Apple item id (its App Store Connect "Apple ID"), as digits.
    #[serde(rename = "appleItemId")]
    pub apple_item_id: String,
    #[serde(default, rename = "appleVersionId")]
    pub apple_version_id: String,
    /// The package's base URL (or its `manifest.json`).
    #[serde(default)]
    pub url: String,
    /// An approximate download size: any one variant's.
    #[serde(default)]
    pub size: i64,
    #[serde(default, rename = "requiredDeviceCapabilities")]
    pub required_device_capabilities: Vec<String>,
    #[serde(default, rename = "ageRatingRank")]
    pub age_rating_rank: i64,
    /// The app's page on the marketplace's website.
    #[serde(default, rename = "shareUrl")]
    pub share_url: String,
}

/// Whether `json` is an App Index document (by its `$schema`).
pub fn sniff(json: &serde_json::Value) -> bool {
    json.get("$schema")
        .and_then(|s| s.as_str())
        .is_some_and(|s| s.starts_with(SCHEMA_PREFIX))
}

/// `sha256:<hex>` → `<hex>`, lowercased; anything else passes through trimmed.
fn hex_digest(d: &str) -> String {
    d.trim()
        .strip_prefix("sha256:")
        .unwrap_or(d.trim())
        .to_ascii_lowercase()
}

/// Join a possibly relative `location` onto `base`; absolute URLs pass through.
pub fn resolve(base: &str, location: &str) -> String {
    if location.is_empty() || location.contains("://") || base.is_empty() {
        return location.to_string();
    }
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        location.trim_start_matches('/')
    )
}

/// Unix seconds for an ISO 8601 date or date-time (`2026-05-01`, `2026-05-01T18:00:00Z`,
/// `2026-05-01T18:00:00.123-06:00`), or `0`.
pub fn unix_seconds(iso: &str) -> i64 {
    let s = iso.trim();
    let (date, time) = s.split_once(['T', ' ']).unwrap_or((s, ""));
    let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (Some(Some(y)), Some(Some(m)), Some(Some(day))) = (d.next(), d.next(), d.next()) else {
        return 0;
    };
    // Days from the civil date (Howard Hinnant's algorithm).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let (clock, offset) = match time.find(['Z', '+', '-']) {
        Some(i) => (&time[..i], &time[i..]),
        None => (time, ""),
    };
    let mut c = clock.split(':').map(|p| {
        p.split('.')
            .next()
            .and_then(|n| n.parse::<i64>().ok())
            .unwrap_or(0)
    });
    let secs = c.next().unwrap_or(0) * 3600 + c.next().unwrap_or(0) * 60 + c.next().unwrap_or(0);
    let tz = match offset.chars().next() {
        Some(sign @ ('+' | '-')) => {
            let mut o = offset[1..]
                .split(':')
                .map(|p| p.parse::<i64>().unwrap_or(0));
            let mins = o.next().unwrap_or(0) * 60 + o.next().unwrap_or(0);
            if sign == '+' { mins * 60 } else { -mins * 60 }
        }
        _ => 0,
    };
    days * 86_400 + secs - tz
}

/// The leading integer of a build number (`"207"` → 207, `"1.4.2"` → 1), for ordering.
pub fn leading_int(s: &str) -> i64 {
    s.trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}

/// The rows and labels this document imports for `platform` in `locale`.
pub fn import(doc: &AppIndex, repo: u64, locale: &str, platform: &str, stamp: i64) -> Import {
    let info = doc.catalog.as_ref();
    let rank: BTreeMap<&str, i64> = info
        .map(|c| {
            c.rank
                .iter()
                .enumerate()
                .map(|(i, n)| (n.as_str(), i as i64))
                .collect()
        })
        .unwrap_or_default();
    let text = |m: &Localized<String>| pick(m, locale).cloned().unwrap_or_default();
    let labels = |m: &BTreeMap<String, Label>| -> Vec<(String, String, String)> {
        m.iter()
            .map(|(k, l)| {
                let name = pick(&l.name, locale).cloned().unwrap_or_else(|| k.clone());
                (k.clone(), name, text(&l.description))
            })
            .collect()
    };
    let apps = doc
        .apps
        .iter()
        .filter_map(|a| {
            let p = a.platforms.get(platform)?;
            build(
                a,
                p,
                repo,
                locale,
                platform,
                rank.get(a.name.as_str()).copied(),
            )
        })
        .collect();
    Import {
        name: info.map(|c| text(&c.name)).unwrap_or_default(),
        stamp: match unix_seconds(&doc.generated) {
            0 => stamp,
            t => t,
        },
        mirrors: info.map(|c| c.mirrors.clone()).unwrap_or_default(),
        categories: info
            .map(|c| labels(&c.categories))
            .unwrap_or_default()
            .into_iter()
            .map(|(k, n, _)| (k, n))
            .collect(),
        anti_features: info.map(|c| labels(&c.anti_features)).unwrap_or_default(),
        apps,
    }
}

fn build(
    a: &AppEntry,
    p: &Platform,
    repo: u64,
    locale: &str,
    platform: &str,
    rank: Option<i64>,
) -> Option<App> {
    // Platform-level text wins over the app-level default, locale by locale.
    let text = |plat: &Localized<String>, app: &Localized<String>| {
        pick(plat, locale)
            .or_else(|| pick(app, locale))
            .cloned()
            .unwrap_or_default()
    };
    let pkg = match platform {
        "ios" => p.bundle_identifier.clone(),
        _ => p.application_id.clone(),
    };
    if pkg.is_empty() {
        return None;
    }
    let assets = &a.source.assets;
    let mut app = App {
        id: app_key(repo, &pkg),
        repo: day_persistence::prelude::One::to(repo),
        platform: platform.to_string(),
        pkg: pkg.clone(),
        name: Some(text(&p.title, &a.title))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| a.name.clone()),
        summary: text(&p.subtitle, &a.subtitle),
        description: text(&p.description, &a.description),
        license: a.source.license.clone(),
        author: a.developer.name.clone(),
        author_website: a.developer.url.clone(),
        website: a
            .links
            .get("website")
            .or_else(|| a.links.get("marketing"))
            .and_then(|l| pick(l, locale))
            .cloned()
            .unwrap_or_default(),
        source_code: a.source.url.clone(),
        icon_path: p
            .assets
            .icon
            .as_ref()
            .map(|i| resolve(assets, &i.location))
            .unwrap_or_default(),
        categories: a.categories.clone(),
        screenshots: pick(&p.assets.screenshots, locale)
            .map(|shots| shots.iter().map(|s| resolve(assets, &s.location)).collect())
            .unwrap_or_default(),
        last_updated: unix_seconds(&p.released),
        version_name: p.version.clone(),
        version_code: leading_int(&p.build_number),
        bundle_version: p.build_number.clone(),
        min_os: p.minimum_system_version.clone(),
        permissions: p.permissions.iter().map(|x| x.key.clone()).collect(),
        anti_features: a.anti_features.clone(),
        whats_new: text(&p.release_notes, &a.release_notes),
        rank: rank.unwrap_or(crate::catalog::UNRANKED),
        ..Default::default()
    };
    if let Some(apk) = &p.distribution.apk {
        app.apk_path = resolve(&a.source.release, &apk.url);
        app.apk_size = apk.size;
        app.apk_sha256 = hex_digest(&apk.digest);
        app.signer = hex_digest(&apk.signer);
        app.min_sdk = apk.min_sdk;
        if apk.version_code > 0 {
            app.version_code = apk.version_code;
        }
    }
    if let Some(adp) = &p.distribution.adp {
        app.apple_item_id = adp.apple_item_id.trim().parse().unwrap_or(0);
        app.apple_version_id = adp.apple_version_id.trim().parse().unwrap_or(0);
        app.package_url = adp.url.clone();
        app.apk_size = adp.size;
        app.device_caps = adp.required_device_capabilities.clone();
        app.age_rank = adp.age_rating_rank;
        app.share_url = adp.share_url.clone();
    }
    // An iOS entry the marketplace cannot install (no Apple item id: an App Store-only app) is a
    // landing-page listing, not a store one.
    if platform == "ios" && app.apple_item_id == 0 {
        return None;
    }
    Some(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = include_str!("testdata/appindex.json");

    fn doc() -> AppIndex {
        serde_json::from_str(DOC).unwrap()
    }

    #[test]
    fn recognizes_the_schema() {
        let v: serde_json::Value = serde_json::from_str(DOC).unwrap();
        assert!(sniff(&v));
        assert!(!sniff(&serde_json::json!({"repo": {}, "packages": {}})));
    }

    #[test]
    fn imports_the_ios_platform_with_marketplace_fields() {
        let import = import(&doc(), 7, "fr-FR", "ios", 0);
        assert_eq!(import.name, "Catalogue App Fair");
        assert_eq!(import.stamp, 1_777_658_400);
        assert_eq!(import.mirrors, vec!["https://mirror.example/appindex"]);
        assert_eq!(
            import.categories,
            vec![("internet".to_string(), "Internet".to_string())]
        );
        // The App Store-only entry (no `adp`) is not a marketplace listing.
        assert_eq!(import.apps.len(), 1);
        let a = &import.apps[0];
        assert_eq!(a.id, "7~org.appfair.app.Net-Skip");
        assert_eq!(a.platform, "ios");
        assert_eq!(a.name, "Net Skip");
        // The platform-level French subtitle wins over the app-level one.
        assert_eq!(a.summary, "Un navigateur web humain");
        assert_eq!(a.description, "Le navigateur.");
        assert_eq!(a.version_name, "1.4.23");
        assert_eq!(a.bundle_version, "236");
        assert_eq!(a.version_code, 236);
        assert_eq!(a.apple_item_id, 1640618584);
        assert_eq!(a.apple_version_id, 869899373);
        assert_eq!(a.package_url, "https://packages.appfair.cloud/1640618584/");
        assert_eq!(a.min_os, "17.0");
        assert_eq!(a.device_caps, vec!["arm64"]);
        assert_eq!(
            a.icon_path,
            "https://raw.example/Net-Skip/1.4.23/Darwin/icon.png"
        );
        assert_eq!(a.screenshots.len(), 1);
        assert_eq!(a.permissions, vec!["NSCameraUsageDescription"]);
        assert_eq!(a.author, "App Fair Project");
        assert_eq!(a.categories, vec!["internet"]);
        assert_eq!(a.rank, 0);
        assert_eq!(a.last_updated, 1_777_550_400);
    }

    #[test]
    fn imports_the_android_platform_with_its_apk() {
        let import = import(&doc(), 7, "en-US", "android", 0);
        assert_eq!(import.apps.len(), 2);
        let a = import
            .apps
            .iter()
            .find(|a| a.pkg == "org.appfair.app.Net_Skip")
            .unwrap();
        assert_eq!(a.platform, "android");
        assert_eq!(a.summary, "A humane web browser");
        assert_eq!(a.version_code, 228);
        assert_eq!(
            a.apk_path,
            "https://github.com/Net-Skip/Net-Skip/releases/download/1.4.23/Net-Skip-release.apk"
        );
        assert_eq!(a.apk_sha256, "ab12");
        assert_eq!(a.signer, "cd34");
        assert_eq!(a.min_sdk, 29);
        // Unranked apps sort after ranked ones.
        let other = import
            .apps
            .iter()
            .find(|a| a.pkg != "org.appfair.app.Net_Skip")
            .unwrap();
        assert_eq!(other.rank, crate::catalog::UNRANKED);
    }

    #[test]
    fn parses_iso_dates() {
        assert_eq!(unix_seconds("1970-01-01T00:00:00Z"), 0);
        assert_eq!(unix_seconds("2026-05-01"), 1_777_593_600);
        assert_eq!(unix_seconds("2026-05-01T18:00:00Z"), 1_777_658_400);
        assert_eq!(unix_seconds("2026-05-01T12:00:00-06:00"), 1_777_658_400);
        assert_eq!(unix_seconds("2026-05-01T18:00:00.250Z"), 1_777_658_400);
        assert_eq!(unix_seconds("garbage"), 0);
    }

    #[test]
    fn resolves_asset_locations() {
        assert_eq!(
            resolve("https://a.example/x/", "/i.png"),
            "https://a.example/x/i.png"
        );
        assert_eq!(
            resolve("https://a.example/x", "https://b/i.png"),
            "https://b/i.png"
        );
        assert_eq!(resolve("", "i.png"), "i.png");
    }
}
