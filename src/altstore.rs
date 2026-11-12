// Copyright © The Daybrite Project
// SPDX-License-Identifier: MPL-2.0

//! AltStore sources (https://faq.altstore.io/developers/make-a-source), for iOS. The format
//! AltStore PAL and other EU marketplaces read: a source with a list of apps, each with its
//! versions newest first.
//!
//! Only apps with a `marketplaceID` (the app's Apple item id, which notarized apps have) can be
//! installed through MarketplaceKit, so those are the ones App Fair imports; an AltStore Classic
//! app (a sideloaded IPA) has none and is skipped. A version's `downloadURL` points at its
//! alternative distribution package (the folder or its `manifest.json`); it is used as is when it
//! is on App Fair's marketplace domain, and the marketplace server's copy is used otherwise
//! (`app_fair_marketplace::package_url`).

#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::appindex::{leading_int, unix_seconds};
use crate::catalog::{App, Import, app_key};

#[derive(Debug, Deserialize)]
pub struct Source {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub apps: Vec<SourceApp>,
    /// Bundle identifiers to feature, in order.
    #[serde(default, rename = "featuredApps")]
    pub featured_apps: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SourceApp {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "bundleIdentifier")]
    pub bundle_identifier: String,
    /// The app's Apple item id, as AltStore PAL requires for notarized apps.
    #[serde(default, rename = "marketplaceID")]
    pub marketplace_id: Option<serde_json::Value>,
    #[serde(default, rename = "developerName")]
    pub developer_name: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default, rename = "localizedDescription")]
    pub localized_description: String,
    #[serde(default, rename = "iconURL")]
    pub icon_url: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub screenshots: Screenshots,
    /// Before `screenshots`, sources listed plain URLs here.
    #[serde(default, rename = "screenshotURLs")]
    pub screenshot_urls: Vec<String>,
    #[serde(default)]
    pub versions: Vec<Version>,
    #[serde(default, rename = "appPermissions")]
    pub app_permissions: Permissions,
    // The pre-`versions` layout carried one version on the app itself.
    #[serde(default)]
    pub version: String,
    #[serde(default, rename = "versionDate")]
    pub version_date: String,
    #[serde(default, rename = "versionDescription")]
    pub version_description: String,
    #[serde(default, rename = "downloadURL")]
    pub download_url: String,
    #[serde(default)]
    pub size: i64,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Version {
    #[serde(default)]
    pub version: String,
    #[serde(default, rename = "buildVersion")]
    pub build_version: String,
    #[serde(default, rename = "marketingVersion")]
    pub marketing_version: String,
    #[serde(default)]
    pub date: String,
    #[serde(default, rename = "localizedDescription")]
    pub localized_description: String,
    #[serde(default, rename = "downloadURL")]
    pub download_url: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default, rename = "minOSVersion")]
    pub min_os_version: String,
}

/// Screenshots come as a list (of URLs or `{imageURL, width, height}` objects) or as
/// `{iphone: [...], ipad: [...]}`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Screenshots {
    List(Vec<Shot>),
    ByDevice(BTreeMap<String, Vec<Shot>>),
}

impl Default for Screenshots {
    fn default() -> Self {
        Screenshots::List(Vec::new())
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Shot {
    Url(String),
    Image {
        #[serde(rename = "imageURL")]
        image_url: String,
    },
}

impl Shot {
    fn url(&self) -> &str {
        match self {
            Shot::Url(u) => u,
            Shot::Image { image_url } => image_url,
        }
    }
}

/// `appPermissions`. `privacy` is documented as a map of usage-description keys to text, and
/// older sources wrote a list of `{name, usageDescription}`; both are read.
#[derive(Debug, Deserialize, Default)]
pub struct Permissions {
    #[serde(default)]
    pub privacy: Privacy,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Privacy {
    Map(BTreeMap<String, String>),
    List(Vec<NamedPrivacy>),
}

impl Default for Privacy {
    fn default() -> Self {
        Privacy::Map(BTreeMap::new())
    }
}

#[derive(Debug, Deserialize)]
pub struct NamedPrivacy {
    pub name: String,
}

/// AltStore's fixed categories, with the names App Fair shows for them.
const CATEGORIES: &[(&str, &str)] = &[
    ("developer", "Developer"),
    ("entertainment", "Entertainment"),
    ("games", "Games"),
    ("lifestyle", "Lifestyle"),
    ("other", "Other"),
    ("photo-video", "Photo & Video"),
    ("social", "Social"),
    ("utilities", "Utilities"),
];

/// Whether `json` looks like an AltStore source: a top-level `apps` list whose entries carry a
/// `bundleIdentifier`.
pub fn sniff(json: &serde_json::Value) -> bool {
    json.get("apps")
        .and_then(|a| a.as_array())
        .is_some_and(|apps| {
            apps.iter()
                .all(|a| a.get("bundleIdentifier").is_some_and(|b| b.is_string()))
        })
        && json.get("$schema").is_none()
}

/// The rows and labels this source imports. Everything is iOS: on any other platform the source
/// lists nothing installable.
pub fn import(source: &Source, repo: u64, platform: &str, stamp: i64) -> Import {
    let featured: BTreeMap<&str, i64> = source
        .featured_apps
        .iter()
        .enumerate()
        .map(|(i, b)| (b.as_str(), i as i64))
        .collect();
    let apps = if platform == "ios" {
        source
            .apps
            .iter()
            .filter_map(|a| build(a, repo, featured.get(a.bundle_identifier.as_str()).copied()))
            .collect()
    } else {
        Vec::new()
    };
    Import {
        name: source.name.clone(),
        stamp,
        mirrors: Vec::new(),
        categories: CATEGORIES
            .iter()
            .map(|(k, n)| (k.to_string(), n.to_string()))
            .collect(),
        anti_features: Vec::new(),
        apps,
    }
}

fn build(a: &SourceApp, repo: u64, rank: Option<i64>) -> Option<App> {
    let item: u64 = match a.marketplace_id.as_ref()? {
        serde_json::Value::String(s) => s.trim().parse().ok()?,
        serde_json::Value::Number(n) => n.as_u64()?,
        _ => return None,
    };
    let latest = a.versions.first().cloned().unwrap_or_else(|| Version {
        version: a.version.clone(),
        date: a.version_date.clone(),
        localized_description: a.version_description.clone(),
        download_url: a.download_url.clone(),
        size: a.size,
        ..Default::default()
    });
    let screenshots: Vec<String> = match &a.screenshots {
        Screenshots::List(l) if !l.is_empty() => l.iter().map(|s| s.url().to_string()).collect(),
        Screenshots::ByDevice(m) => m
            .get("iphone")
            .or_else(|| m.values().next())
            .map(|l| l.iter().map(|s| s.url().to_string()).collect())
            .unwrap_or_default(),
        _ => a.screenshot_urls.clone(),
    };
    let permissions = match &a.app_permissions.privacy {
        Privacy::Map(m) => m.keys().cloned().collect(),
        Privacy::List(l) => l.iter().map(|p| privacy_key(&p.name)).collect(),
    };
    let category = if a.category.is_empty() {
        "other".to_string()
    } else {
        a.category.clone()
    };
    Some(App {
        id: app_key(repo, &a.bundle_identifier),
        repo: day_persistence::prelude::One::to(repo),
        platform: "ios".to_string(),
        pkg: a.bundle_identifier.clone(),
        name: if a.name.is_empty() {
            a.bundle_identifier.clone()
        } else {
            a.name.clone()
        },
        summary: a.subtitle.clone(),
        description: a.localized_description.clone(),
        author: a.developer_name.clone(),
        icon_path: a.icon_url.clone(),
        categories: vec![category],
        screenshots,
        last_updated: unix_seconds(&latest.date),
        version_name: if latest.marketing_version.is_empty() {
            latest.version.clone()
        } else {
            latest.marketing_version.clone()
        },
        version_code: leading_int(&latest.build_version),
        bundle_version: latest.build_version.clone(),
        apk_size: latest.size,
        min_os: latest.min_os_version.clone(),
        permissions,
        whats_new: latest.localized_description.clone(),
        apple_item_id: item,
        package_url: latest.download_url.clone(),
        rank: rank.unwrap_or(crate::catalog::UNRANKED),
        ..Default::default()
    })
}

/// The Info.plist key an older source's short privacy name stands for: `Camera` →
/// `NSCameraUsageDescription`.
fn privacy_key(name: &str) -> String {
    if name.starts_with("NS") {
        name.to_string()
    } else {
        format!("NS{name}UsageDescription")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = include_str!("testdata/altstore-source.json");

    #[test]
    fn recognizes_a_source() {
        let v: serde_json::Value = serde_json::from_str(SOURCE).unwrap();
        assert!(sniff(&v));
        assert!(!crate::appindex::sniff(&v));
    }

    #[test]
    fn imports_marketplace_apps_only() {
        let source: Source = serde_json::from_str(SOURCE).unwrap();
        let import = import(&source, 3, "ios", 42);
        assert_eq!(import.name, "Example PAL Source");
        assert_eq!(import.stamp, 42);
        // The Classic app without a marketplaceID is not installable through MarketplaceKit.
        assert_eq!(import.apps.len(), 2);
        let a = &import.apps[0];
        assert_eq!(a.id, "3~com.example.notes");
        assert_eq!(a.platform, "ios");
        assert_eq!(a.apple_item_id, 6476788646);
        assert_eq!(a.version_name, "1.2 (Beta)");
        assert_eq!(a.bundle_version, "12");
        assert_eq!(a.version_code, 12);
        assert_eq!(a.min_os, "17.4");
        assert_eq!(
            a.package_url,
            "https://example.com/adp/6476788646/manifest.json"
        );
        assert_eq!(a.whats_new, "Faster sync.");
        assert_eq!(a.categories, vec!["utilities"]);
        assert_eq!(
            a.screenshots,
            vec![
                "https://example.com/shots/1.png",
                "https://example.com/shots/2.png"
            ]
        );
        assert_eq!(
            a.permissions,
            vec!["NSCameraUsageDescription", "NSMicrophoneUsageDescription"]
        );
        // Featured apps rank first, in the source's order.
        assert_eq!(a.rank, 0);
        assert_eq!(a.last_updated, 1_777_593_600);

        // A numeric marketplaceID, the legacy single-version layout, and a privacy list.
        let b = &import.apps[1];
        assert_eq!(b.apple_item_id, 1640618584);
        assert_eq!(b.version_name, "2.0");
        assert_eq!(b.package_url, "https://appfair.net/adp/1640618584/");
        assert_eq!(b.permissions, vec!["NSLocationWhenInUseUsageDescription"]);
        assert_eq!(b.categories, vec!["other"]);
        assert_eq!(b.rank, crate::catalog::UNRANKED);
    }

    #[test]
    fn lists_nothing_off_ios() {
        let source: Source = serde_json::from_str(SOURCE).unwrap();
        assert!(import(&source, 3, "android", 0).apps.is_empty());
    }
}
