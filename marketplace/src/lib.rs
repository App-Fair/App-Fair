// Copyright © The Daybrite Project
// SPDX-License-Identifier: MPL-2.0

//! App Fair as an alternative app marketplace on iOS, through Apple's MarketplaceKit.
//!
//! Android installs a downloaded APK through `PackageInstaller`; iOS has no such thing. A
//! marketplace instead describes an app to the system (its Apple item id and the URL of its
//! *alternative distribution package*, which App Store Connect delivers to the marketplace's
//! server after notarization), and the system downloads, licenses and installs it. Three rules
//! shape this crate:
//!
//! - **Only a tap installs.** The system accepts an install only from MarketplaceKit's own
//!   `ActionButton`, so [`action_button`] embeds that control (through day-piece-swiftui) instead
//!   of App Fair drawing its own. The same control launches and deletes apps.
//! - **The server vouches for every install.** When the person taps, the button asks the
//!   marketplace server for an *install verification token*, a JWS signed with the marketplace's
//!   private key (docs/marketplace-server.md; `app-fair-marketplace-token` is the reference
//!   signer).
//! - **Packages live on the marketplace's domain.** The system only fetches a package from the
//!   domain registered with App Store Connect, so [`package_url`] maps a catalog's package URL
//!   onto the server when it points anywhere else.
//!
//! Everything the system reports comes back through [`observe`]: snapshots of the installed and
//! installing apps (with each installation's progress and error), and what each button's
//! confirmation did. The app turns those into the same install sheet Android shows.
//!
//! Off iOS every call answers [`day_bridge::Error::Unsupported`] (the Rust fallback arm), so the
//! crate builds and tests everywhere.

use serde::{Deserialize, Serialize};

pub use day_bridge::{Error, Support};

/// App Fair's marketplace API (appfair.cloud's Worker): the install token exchange
/// (docs/marketplace-server.md).
pub const DEFAULT_SERVER: &str = "https://api.appfair.cloud";

/// Where every alternative distribution package App Fair distributes is downloaded from:
/// `<base>/<appleItemId>/`, which redirects to the files on the app's own GitHub release.
pub const DEFAULT_PACKAGES: &str = "https://packages.appfair.cloud";

/// The domain App Store Connect knows as App Fair's marketplace domain (registered 2026-11-12). A
/// package URL must be on it, or on one of its subdomains, for the system to download from it.
pub const DEFAULT_DOMAIN: &str = "appfair.cloud";

/// The App Group the app shares with its marketplace extension.
pub const DEFAULT_APP_GROUP: &str = "group.org.appfair.AppFair";

/// Where the marketplace server lives, and the App Group shared with the extension.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub server: String,
    pub app_group: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: DEFAULT_SERVER.to_string(),
            app_group: DEFAULT_APP_GROUP.to_string(),
        }
    }
}

/// What MarketplaceKit reports about this device.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Who distributed App Fair itself: `appStore`, `testFlight`, `marketplace:<id>`, `web`,
    /// `other`. `None` when the system could not say.
    pub distributor: Option<String>,
    /// The country the device's marketplace catalog is for (iOS 26.2 and later).
    pub catalog_region: Option<String>,
    /// The highest age rating this device may install (iOS 26 and later).
    pub maximum_age_rating: Option<i64>,
    pub error: Option<String>,
}

/// The regions where Apple allows alternative app marketplaces: the European Union's member
/// states, Japan, and Brazil (MarketplaceKit "Participating in alternative distribution for
/// specific regions"). Alpha-2 and alpha-3 codes, since the system's region string is either.
const MARKETPLACE_REGIONS: &[&str] = &[
    "AT", "AUT", "BE", "BEL", "BG", "BGR", "HR", "HRV", "CY", "CYP", "CZ", "CZE", "DK", "DNK",
    "EE", "EST", "FI", "FIN", "FR", "FRA", "DE", "DEU", "GR", "GRC", "HU", "HUN", "IE", "IRL",
    "IT", "ITA", "LV", "LVA", "LT", "LTU", "LU", "LUX", "MT", "MLT", "NL", "NLD", "PL", "POL",
    "PT", "PRT", "RO", "ROU", "SK", "SVK", "SI", "SVN", "ES", "ESP", "SE", "SWE", "JP", "JPN",
    "BR", "BRA",
];

impl Status {
    /// `Some(false)` when the device's catalog region is known and Apple allows no alternative
    /// marketplaces there; `None` when the region is unknown (before iOS 26.2), in which case the
    /// system itself refuses an install it cannot honor.
    pub fn region_supported(&self) -> Option<bool> {
        let region = self.catalog_region.as_deref()?.trim().to_ascii_uppercase();
        Some(MARKETPLACE_REGIONS.contains(&region.as_str()))
    }
}

/// An installed app's version, as the system records it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVersion {
    /// `CFBundleVersion`.
    pub version: String,
    /// `CFBundleShortVersionString`.
    pub short_version: String,
    pub apple_version_id: u64,
    pub account: Option<String>,
}

/// An installation's progress.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub fraction: f64,
    pub completed: i64,
    pub total: i64,
    pub cancellable: bool,
}

/// One app the system knows this marketplace installed, or is installing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryApp {
    /// The Apple item id.
    pub id: u64,
    pub installed: Option<InstalledVersion>,
    pub installing: bool,
    pub updating: bool,
    pub progress: Option<Progress>,
    /// The last installation's failure (iOS 18 and later).
    pub error: Option<String>,
}

/// What [`observe`] delivers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    /// The marketplace's apps, after any change, and four times a second while one installs.
    Library {
        #[serde(default)]
        loading: bool,
        #[serde(default)]
        apps: Vec<LibraryApp>,
    },
    /// The person tapped a button; its token request is under way.
    Confirming {
        #[serde(default)]
        ids: Vec<u64>,
    },
    /// The server vouched for these installs; the system takes it from here.
    Confirmed {
        #[serde(default)]
        ids: Vec<u64>,
    },
    /// The install did not start: the server declined, or could not be reached.
    Declined {
        #[serde(default)]
        ids: Vec<u64>,
        message: Option<String>,
    },
}

/// One app a button installs or updates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub apple_item_id: u64,
    /// The version the catalog offers, when it knows it; it speeds up the install.
    pub apple_version_id: Option<u64>,
    pub bundle_id: String,
    /// The alternative distribution package's base URL, from [`package_url`].
    pub package_url: String,
    /// The app's page on the marketplace's website, offered in the Home Screen share sheet.
    pub share_url: Option<String>,
    /// Whether this replaces an installed version.
    pub update: bool,
}

/// What a button does when tapped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Install(Target),
    /// Several installs or updates under one confirmation: "Update all".
    Batch(Vec<Target>),
    /// Open an installed app.
    Launch(u64),
    /// Delete an installed app, after the system's own confirmation.
    Delete(u64),
}

/// How a button looks. Colors are `#rrggbb` or `#rrggbbaa`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

/// A MarketplaceKit `ActionButton`: what it does, its label, and the account and server its
/// install confirmation uses.
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonSpec {
    pub action: Action,
    pub label: String,
    pub server: String,
    pub account: String,
    pub style: Style,
}

/// The provider params `MarketplaceBridge.swift` decodes (`AppFairButtonSpec`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ButtonParams<'a> {
    kind: &'static str,
    label: &'a str,
    server: &'a str,
    account: &'a str,
    targets: Vec<&'a Target>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item: Option<u64>,
    style: &'a Style,
}

impl ButtonSpec {
    /// The params JSON the Swift provider decodes.
    pub fn params(&self) -> String {
        let (kind, targets, item) = match &self.action {
            Action::Install(t) => ("install", vec![t], None),
            Action::Batch(ts) => ("batch", ts.iter().collect(), None),
            Action::Launch(id) => ("launch", Vec::new(), Some(*id)),
            Action::Delete(id) => ("delete", Vec::new(), Some(*id)),
        };
        serde_json::to_string(&ButtonParams {
            kind,
            label: &self.label,
            server: &self.server,
            account: &self.account,
            targets,
            item,
            style: &self.style,
        })
        .unwrap_or_default()
    }
}

/// The view name `MarketplaceBridge.swift` registers its provider under.
pub const BUTTON_VIEW: &str = "appfair_marketplace_action";

/// MarketplaceKit's `ActionButton` for `spec`, as a Day piece. It grows like any hosted SwiftUI
/// leaf, so give it a frame. Off iOS it is day-piece-swiftui's placeholder; check [`support`].
pub fn action_button(spec: &ButtonSpec) -> day_piece_swiftui::SwiftUi {
    day_piece_swiftui::swiftui(BUTTON_VIEW).params(spec.params())
}

/// What the catalog says an app needs from the device, in the shape of MarketplaceKit's
/// `InstallRequirements`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirements {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_system_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_device_capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_rating_rank: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_install_size: Option<u64>,
}

/// What the marketplace extension reads from the App Group container: who this device is to the
/// server, and the newest version the catalogs offer for each app, so the system's automatic
/// updates can run while App Fair is not.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shared {
    pub server: String,
    pub account: String,
    pub apps: Vec<SharedApp>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedApp {
    pub apple_item_id: u64,
    pub apple_version_id: Option<u64>,
    pub bundle_id: String,
    pub package_url: String,
    pub bundle_version: String,
    pub short_version: String,
    pub share_url: Option<String>,
}

/// The package URL the system will accept for an app: `catalog_url` when it is on `domain` (or a
/// subdomain of it) over HTTPS, else the marketplace's canonical one under `packages`, where
/// every package App Store Connect delivers to App Fair is published (docs/marketplace-server.md
/// "Packages").
pub fn package_url(
    packages: &str,
    domain: &str,
    catalog_url: Option<&str>,
    apple_item_id: u64,
) -> String {
    if let Some(url) = catalog_url.map(str::trim).filter(|u| on_domain(u, domain)) {
        return if url.ends_with('/') {
            url.to_string()
        } else if let Some(base) = url.strip_suffix("manifest.json") {
            // AltStore PAL sources may point at the manifest itself; the system wants its folder.
            base.to_string()
        } else {
            format!("{url}/")
        };
    }
    format!("{}/{apple_item_id}/", packages.trim_end_matches('/'))
}

/// Whether `url` is an `https` URL whose host is `domain` or a subdomain of it.
pub fn on_domain(url: &str, domain: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("");
    let host = host.split(':').next().unwrap_or("").to_ascii_lowercase();
    let domain = domain.to_ascii_lowercase();
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// Whether this target installs through MarketplaceKit at all: `Native` on iOS, `Unsupported`
/// everywhere else.
pub fn support() -> Support {
    account_native_support()
}

/// Point the bridge at the marketplace server and App Group. Call once at launch, before any
/// other call.
pub fn configure(config: &Config) -> Result<(), Error> {
    configure_native(&serde_json::to_string(config).unwrap_or_default())
}

/// This device's anonymous marketplace account: a random id kept in the keychain, created on
/// first use.
pub async fn account() -> Result<String, Error> {
    account_native_future().await
}

/// The system's view of the marketplace on this device.
pub async fn status() -> Result<Status, Error> {
    let json = status_native_future().await?;
    serde_json::from_str(&json).map_err(|e| Error::Foreign(e.to_string()))
}

/// Listen for library snapshots and button events, on whatever thread the system delivers them.
/// The first snapshot arrives right away. Returns the token [`stop_observing`] takes, or
/// [`Error::Unsupported`] off iOS.
pub fn observe(mut on_event: impl FnMut(Event) + Send + 'static) -> Result<u64, Error> {
    observe_native_stream(move |item| {
        if let day_bridge::Item::Value(json) = item {
            match serde_json::from_str::<Event>(&json) {
                Ok(event) => on_event(event),
                Err(e) => eprintln!("app-fair-marketplace: unreadable event ({e}): {json}"),
            }
        }
    })
}

/// Stop the listener [`observe`] returned.
pub fn stop_observing(token: u64) {
    observe_native_stop(token);
}

/// Whether this device meets `requirements`, by the installer's own rules. Off iOS, `Ok(true)`:
/// there is nothing to compare against.
pub async fn satisfies(requirements: &Requirements) -> Result<bool, Error> {
    let json = serde_json::to_string(requirements).unwrap_or_default();
    match satisfies_native_future(&json).await {
        Err(Error::Unsupported) => Ok(true),
        other => other.map(|met| met != 0),
    }
}

/// Cancel `apple_item_id`'s installation, where the system allows it.
pub fn cancel(apple_item_id: u64) -> Result<(), Error> {
    cancel_native(apple_item_id as i64)
}

/// Hand the marketplace extension the account, the server and the catalogs' newest versions.
pub fn share(shared: &Shared) -> Result<(), Error> {
    share_native(&serde_json::to_string(shared).unwrap_or_default())
}

day_bridge::bridge! {
    #[day_bridge::declare]
    extern "day" {
        /// Set the server and App Group (a JSON [`Config`]).
        fn configure_native(config: &str) -> Result<(), day_bridge::Error>;
        /// Completes with a JSON [`Status`].
        fn status_native(done: day_bridge::Done<String>) -> Result<(), day_bridge::Error>;
        /// Completes with the anonymous account id, created on first use.
        fn account_native(done: day_bridge::Done<String>) -> Result<(), day_bridge::Error>;
        /// JSON [`Event`]s: library snapshots and button confirmations.
        fn observe_native(emit: day_bridge::Emit<String>) -> Result<(), day_bridge::Error>;
        /// Completes with `1` when the device meets JSON [`Requirements`], else `0`.
        fn satisfies_native(requirements: &str, done: day_bridge::Done<i32>) -> Result<(), day_bridge::Error>;
        /// Cancel an installation in progress.
        fn cancel_native(apple_item_id: i64) -> Result<(), day_bridge::Error>;
        /// Write a JSON [`Shared`] where the marketplace extension reads it.
        fn share_native(shared: &str) -> Result<(), day_bridge::Error>;
    }

    #[day_bridge::impl(swift, platforms = [ios], src = "platform/ios/MarketplaceBridge.swift")]

    #[day_bridge::impl(rust, platforms = [other])]
    fn configure_native(_config: &str) -> Result<(), day_bridge::Error> {
        Err(day_bridge::Error::Unsupported)
    }

    #[day_bridge::impl(rust, platforms = [other])]
    fn status_native(_done: day_bridge::Done<String>) -> Result<(), day_bridge::Error> {
        Err(day_bridge::Error::Unsupported)
    }

    #[day_bridge::impl(rust, platforms = [other])]
    fn account_native(_done: day_bridge::Done<String>) -> Result<(), day_bridge::Error> {
        Err(day_bridge::Error::Unsupported)
    }

    #[day_bridge::impl(rust, platforms = [other])]
    fn observe_native(_emit: day_bridge::Emit<String>) -> Result<(), day_bridge::Error> {
        Err(day_bridge::Error::Unsupported)
    }

    #[day_bridge::impl(rust, platforms = [other])]
    fn satisfies_native(_requirements: &str, _done: day_bridge::Done<i32>) -> Result<(), day_bridge::Error> {
        Err(day_bridge::Error::Unsupported)
    }

    #[day_bridge::impl(rust, platforms = [other])]
    fn cancel_native(_apple_item_id: i64) -> Result<(), day_bridge::Error> {
        Err(day_bridge::Error::Unsupported)
    }

    #[day_bridge::impl(rust, platforms = [other])]
    fn share_native(_shared: &str) -> Result<(), day_bridge::Error> {
        Err(day_bridge::Error::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The park/unpark executor a part's future tests use (docs/async.md).
    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};
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

    fn target() -> Target {
        Target {
            apple_item_id: 1640618584,
            apple_version_id: Some(869899373),
            bundle_id: "org.appfair.app.Net-Skip".into(),
            package_url: "https://packages.appfair.cloud/1640618584/".into(),
            share_url: None,
            update: false,
        }
    }

    #[test]
    fn every_call_answers_off_ios() {
        assert_eq!(support(), Support::Unsupported);
        assert_eq!(configure(&Config::default()), Err(Error::Unsupported));
        assert_eq!(block_on(account()), Err(Error::Unsupported));
        assert_eq!(block_on(status()), Err(Error::Unsupported));
        assert_eq!(cancel(1), Err(Error::Unsupported));
        assert_eq!(share(&Shared::default()), Err(Error::Unsupported));
        assert_eq!(block_on(satisfies(&Requirements::default())), Ok(true));
        // The stream fails at once rather than leaving a listener that never hears anything.
        let (tx, rx) = std::sync::mpsc::channel();
        let token = observe(move |e| {
            let _ = tx.send(e);
        });
        assert_eq!(token, Err(Error::Unsupported));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn button_params_match_the_swift_spec() {
        let spec = ButtonSpec {
            action: Action::Install(target()),
            label: "Install".into(),
            server: DEFAULT_SERVER.into(),
            account: "a1".into(),
            style: Style {
                tint: Some("#ffffff".into()),
                width: Some(96.0),
                height: Some(32.0),
                ..Default::default()
            },
        };
        let v: serde_json::Value = serde_json::from_str(&spec.params()).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "kind": "install",
                "label": "Install",
                "server": "https://api.appfair.cloud",
                "account": "a1",
                "targets": [{
                    "appleItemId": 1640618584u64,
                    "appleVersionId": 869899373u64,
                    "bundleId": "org.appfair.app.Net-Skip",
                    "packageUrl": "https://packages.appfair.cloud/1640618584/",
                    "shareUrl": null,
                    "update": false,
                }],
                "style": {"tint": "#ffffff", "width": 96.0, "height": 32.0},
            })
        );

        let launch = ButtonSpec {
            action: Action::Launch(7),
            ..spec.clone()
        };
        let v: serde_json::Value = serde_json::from_str(&launch.params()).unwrap();
        assert_eq!(v["kind"], "launch");
        assert_eq!(v["item"], 7);
        assert_eq!(v["targets"], serde_json::json!([]));

        let batch = ButtonSpec {
            action: Action::Batch(vec![target(), target()]),
            ..spec
        };
        let v: serde_json::Value = serde_json::from_str(&batch.params()).unwrap();
        assert_eq!(v["kind"], "batch");
        assert_eq!(v["targets"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn events_decode_from_the_swift_encoding() {
        let library = r#"{"type":"library","loading":false,"apps":[
            {"id":1640618584,"installed":{"version":"207","shortVersion":"1.3.2",
             "appleVersionId":869899373,"account":"a1"},"installing":false,"updating":true,
             "progress":{"fraction":0.5,"completed":5,"total":10,"cancellable":true}}]}"#;
        let Event::Library { loading, apps } = serde_json::from_str(library).unwrap() else {
            panic!("not a library event");
        };
        assert!(!loading);
        assert_eq!(apps[0].installed.as_ref().unwrap().short_version, "1.3.2");
        assert_eq!(apps[0].progress.as_ref().unwrap().fraction, 0.5);
        assert_eq!(apps[0].error, None);

        let declined = r#"{"type":"declined","ids":[1],"message":"no"}"#;
        assert_eq!(
            serde_json::from_str::<Event>(declined).unwrap(),
            Event::Declined {
                ids: vec![1],
                message: Some("no".into())
            }
        );
        assert_eq!(
            serde_json::from_str::<Event>(r#"{"type":"confirming","ids":[2]}"#).unwrap(),
            Event::Confirming { ids: vec![2] }
        );
    }

    #[test]
    fn packages_resolve_onto_the_marketplace_domain() {
        let packages = "https://packages.appfair.cloud/";
        // A catalog URL on the domain (or a subdomain) is used as the package folder.
        assert_eq!(
            package_url(
                packages,
                "appfair.cloud",
                Some("https://cdn.appfair.cloud/adp/1/manifest.json"),
                1
            ),
            "https://cdn.appfair.cloud/adp/1/"
        );
        assert_eq!(
            package_url(
                packages,
                "appfair.cloud",
                Some("https://appfair.cloud/adp/1"),
                1
            ),
            "https://appfair.cloud/adp/1/"
        );
        // Anywhere else — another host, a look-alike, plain http — falls back to the canonical one.
        for other in [
            "https://github.com/x/releases/download/1/manifest.json",
            "https://appfair.cloud.evil.example/adp/",
            "https://evil.example/@appfair.cloud/",
            "http://appfair.cloud/adp/1/",
            "https://appfair.net/adp/1/",
        ] {
            assert_eq!(
                package_url(packages, "appfair.cloud", Some(other), 42),
                "https://packages.appfair.cloud/42/",
                "{other}"
            );
        }
        assert_eq!(
            package_url(packages, "appfair.cloud", None, 42),
            "https://packages.appfair.cloud/42/"
        );
    }

    #[test]
    fn regions_follow_apples_list() {
        let status = |r: Option<&str>| Status {
            catalog_region: r.map(String::from),
            ..Default::default()
        };
        assert_eq!(status(Some("fr")).region_supported(), Some(true));
        assert_eq!(status(Some("DEU")).region_supported(), Some(true));
        assert_eq!(status(Some("JP")).region_supported(), Some(true));
        assert_eq!(status(Some("US")).region_supported(), Some(false));
        assert_eq!(status(None).region_supported(), None);
    }

    #[test]
    fn requirements_use_apples_keys() {
        let r = Requirements {
            minimum_system_version: Some("17.0".into()),
            required_device_capabilities: Some(vec!["arm64".into()]),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&r).unwrap(),
            serde_json::json!({"minimumSystemVersion": "17.0", "requiredDeviceCapabilities": ["arm64"]})
        );
    }
}
