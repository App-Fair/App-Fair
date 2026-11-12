//! Plain data the UI and storage layers share: the owned values the catalog worker hands back
//! to the pages (`crate::catalog`), deliberately flat and `Clone`, never the raw index JSON.

// These structs mirror the full catalog record; a few fields (APK hash, min-SDK, timestamps) are
// stored for features not yet surfaced in the MVP UI — hash verification, compatibility gating,
// and sort orders.
#![allow(dead_code)]

/// One row in the browse/search list: enough to draw a list cell without touching the detail data.
#[derive(Clone, Debug, PartialEq)]
pub struct AppSummary {
    pub repo_id: u64,
    /// Android application id, e.g. `org.fdroid.fdroid`.
    pub pkg: String,
    pub name: String,
    pub summary: String,
    /// Repo-relative icon path (`/org.fdroid.fdroid/en-US/icon.png`), or empty.
    pub icon_path: String,
    /// Repo base address the icon path resolves against.
    pub repo_address: String,
    /// Newest available version name (for the list's right-hand column).
    pub version_name: String,
    pub version_code: i64,
}

/// A catalog path resolved against its catalog's base: F-Droid paths are repo-relative, while
/// App Index and AltStore catalogs store absolute URLs.
pub fn catalog_url(base: &str, path: &str) -> String {
    if path.contains("://") {
        path.to_string()
    } else {
        format!("{base}{path}")
    }
}

impl AppSummary {
    /// Absolute icon URL, or `None` when the app ships no icon.
    pub fn icon_url(&self) -> Option<String> {
        if self.icon_path.is_empty() {
            None
        } else {
            Some(catalog_url(&self.repo_address, &self.icon_path))
        }
    }
}

/// The detail page's data: everything the store shows about one app.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppDetail {
    pub repo_id: u64,
    pub pkg: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    pub license: String,
    pub author: String,
    /// The author's own website (`authorWebSite`), distinct from the app's `website`.
    pub author_website: String,
    pub website: String,
    pub source_code: String,
    pub icon_path: String,
    pub repo_address: String,
    /// The catalog's mirror base URLs, the APK download's failover (#12).
    pub repo_mirrors: Vec<String>,
    /// Repo-relative screenshot paths.
    pub screenshots: Vec<String>,
    pub version_name: String,
    pub version_code: i64,
    pub apk_path: String,
    pub apk_size: i64,
    pub apk_sha256: String,
    /// SHA-256 of the APK signing certificate the catalog promises (`signer.sha256[0]`), verified
    /// against the downloaded APK before install and shown on the detail page.
    pub signer: String,
    pub min_sdk: i64,
    /// Raw Android permission names from the newest version's manifest.
    pub permissions: Vec<String>,
    /// Anti-feature keys declared by the newest version, resolved to localized name/description.
    pub anti_features: Vec<AntiFeature>,
    pub whats_new: String,
    pub last_updated: i64,
    pub categories: Vec<String>,
    /// `android` or `ios`: which installer this listing is for.
    pub platform: String,
    /// iOS: the offered build's `CFBundleVersion`.
    pub bundle_version: String,
    /// iOS: the oldest system version the offered build runs on.
    pub min_os: String,
    /// iOS: the Apple item id MarketplaceKit installs by; `0` when the app is not installable.
    pub apple_item_id: u64,
    /// iOS: the offered version's Apple version id, `0` when unknown.
    pub apple_version_id: u64,
    /// iOS: the catalog's alternative distribution package URL.
    pub package_url: String,
    /// iOS: device capabilities the build requires.
    pub device_caps: Vec<String>,
    /// iOS: the age rating rank, `0` when unknown.
    pub age_rank: i64,
    /// iOS: the app's page on the marketplace website.
    pub share_url: String,
}

impl AppDetail {
    pub fn icon_url(&self) -> Option<String> {
        if self.icon_path.is_empty() {
            None
        } else {
            Some(catalog_url(&self.repo_address, &self.icon_path))
        }
    }

    pub fn apk_url(&self) -> String {
        catalog_url(&self.repo_address, &self.apk_path)
    }

    pub fn screenshot_urls(&self) -> Vec<String> {
        self.screenshots
            .iter()
            .map(|p| catalog_url(&self.repo_address, p))
            .collect()
    }

    /// Whether this listing installs through the iOS marketplace.
    pub fn is_marketplace(&self) -> bool {
        self.apple_item_id != 0
    }
}

/// A localized anti-feature label, resolved from the repo's `antiFeatures` table.
#[derive(Clone, Debug, PartialEq)]
pub struct AntiFeature {
    pub key: String,
    pub name: String,
    pub description: String,
}

/// A configured repository row.
#[derive(Clone, Debug, PartialEq)]
pub struct RepoRow {
    pub id: u64,
    /// An F-Droid repository's base address without the index filename, e.g.
    /// `https://f-droid.org/repo`; an App Index or AltStore document's URL.
    pub address: String,
    /// The catalog's document format (`crate::catalog::format`), empty before its first sync.
    pub format: String,
    pub name: String,
    /// Repo timestamp of the last successful sync (0 = never synced).
    pub timestamp: i64,
    pub enabled: bool,
    pub app_count: i64,
    /// SHA-256 fingerprint of the repo's signing certificate (empty = unverified repo).
    pub fingerprint: String,
    /// Precedence when the same package is in several repos (higher wins).
    pub priority: i64,
}

/// A category, resolved to its localized name.
#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    pub key: String,
    pub name: String,
}

/// How the catalog list is ordered, chosen from the sort drop-down.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SortOrder {
    /// The catalog's editorial `rank`, falling back to recency for apps a catalog didn't rank (and
    /// for whole catalogs without the `rank` extension). While a search query is active this keeps
    /// relevance first, with rank/recency as the tie-break.
    #[default]
    Default,
    /// Most recently updated first — ignores rank and relevance.
    LastUpdated,
    /// Alphabetical by name, using the device locale's collation (see `crate::collate`).
    Name,
}

/// The address a catalog URL is stored under: an F-Droid repository's base (the URL without a
/// trailing `index-v2.json`, or a URL that names no `.json` file at all), or any other JSON
/// document's own URL. The flag says whether it is an F-Droid repository.
pub fn catalog_address(url: &str) -> (String, bool) {
    let (base, file) = split_index_url(url);
    if file == "index-v2.json" {
        (base, true)
    } else {
        (url.trim().trim_end_matches('/').to_string(), false)
    }
}

/// Split an index URL (`https://host/repo/index-v2.json`) into (base address, index filename).
/// The base is everything up to the final `/`; the filename defaults to `index-v2.json`.
pub fn split_index_url(url: &str) -> (String, String) {
    let trimmed = url.trim().trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some((base, last)) if last.ends_with(".json") => (base.to_string(), last.to_string()),
        _ => (trimmed.to_string(), "index-v2.json".to_string()),
    }
}
