<!--
Copyright © The Daybrite Project
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# App Fair's iOS marketplace: the server contract and go-live checklist

On iOS, App Fair is an **alternative app marketplace** (Apple's program in the European Union,
Japan, and Brazil). The app side is built and tested: `marketplace/` bridges Apple's
MarketplaceKit, `src/market.rs` drives installs, updates, launches and deletions from it, and
`platform/ios/MarketplaceExtension/` is the marketplace extension the system runs. This document
says what the **server** must do for those to work, and what has to happen once Apple approves
App Fair as a marketplace. The server is `appfair/appfair.cloud`: a Cloudflare Worker (`api/`) and
the App License Delivery service as a Cloudflare Container (`license/`); its README covers setup
and deployment. Static pieces (the catalog, the license certificates, the license-resolution
page) are on appfair.net, and package files live on each app's own GitHub release.

Nothing here can be exercised from the United States: installing through a marketplace needs a
device and Apple Account in a supported region, and an approved marketplace entitlement.

## Why a server at all

Android installs a downloaded APK. iOS installs nothing App Fair downloads; the system fetches,
licenses, and installs the app itself, and asks the marketplace to vouch for each step:

1. **Every install carries a token.** When someone taps MarketplaceKit's install button, the
   system calls App Fair's `confirmInstall` closure, which must return an *install verification
   token*: a JWS signed with App Fair's alternative distribution **private key**. That key cannot
   be on the device, so the app asks the server for the token.
2. **Packages come from App Fair's domain.** The system downloads an app's *alternative
   distribution package* (ADP) only from the domain registered as App Fair's marketplace domain
   in App Store Connect.
3. **The system talks to the server directly** for licenses, restores after a device restore, and
   update checks, through endpoints named in `https://<domain>/.well-known/marketplace-kit`.

## How the app is configured

| What | Where | Value |
|---|---|---|
| Marketplace API | `app_fair_marketplace::DEFAULT_SERVER` | `https://api.appfair.cloud` |
| Package base | `app_fair_marketplace::DEFAULT_PACKAGES` | `https://packages.appfair.cloud` |
| Marketplace domain | `app_fair_marketplace::DEFAULT_DOMAIN` | `appfair.cloud` (and its subdomains) |
| Default iOS catalog | `catalog::DEFAULT_APPINDEX_URL` | `https://appfair.net/appindex.v1.json` |
| App Group shared with the extension | `app_fair_marketplace::DEFAULT_APP_GROUP` | `group.org.appfair.AppFair` |
| Staging override | `APP_FAIR_MARKETPLACE` environment variable | any `https://` base URL; its host becomes the domain |

The **account** is anonymous: a random UUID made on the device's first launch, kept in the
keychain (so it survives deleting and reinstalling App Fair, but never syncs to other devices) and
mirrored into the App Group for the extension. App Fair has no sign-in. The account identifies a
device to the server, nothing more.

## The endpoints App Fair calls

### Install tokens: `POST {server}/v1/install-tokens`

Called by the install button when a person confirms an install, update, or "Update all", and by
the marketplace extension for automatic updates. One request names every app that one tap
confirms.

Request headers: `Content-Type: application/json`, `X-AppFair-Account: <account>`.

```json
{
  "account": "8d0f2a1e-6c3b-4f53-9a77-0b6f3f6f1e2c",
  "platform": "iOS",
  "osVersion": "26.2.0",
  "apps": [
    {
      "appleItemId": "1640618584",
      "appleVersionId": "869899373",
      "bundleId": "org.appfair.app.Net-Skip",
      "alternativeDistributionPackage": "https://packages.appfair.cloud/1640618584/",
      "update": false
    }
  ]
}
```

Ids are strings, so no JSON parser rounds them. `appleVersionId` is present when the catalog names
it.

A `200` response holds a token for each app the server vouches for, and a reason for each it
does not:

```json
{
  "tokens": { "1640618584": "eyJhbGciOiJFUzI1NiIsImtpZCI6Ij…" },
  "failures": { "6476788646": "This app is not distributed by App Fair." }
}
```

The app shows the first failure's text in its install sheet, so write it for people. Any other
status, or no answer within 20 seconds, cancels the install with "the marketplace server
answered <status>".

What the server checks before signing:

- the app is one App Fair distributes: its developer chose App Fair in App Store Connect, and a
  package for it has been ingested (see [Packages](#packages));
- `bundleId` matches that app;
- `alternativeDistributionPackage` is a URL the server serves.

What it signs: one token per app, with exactly the claims Apple documents
(["Supplying an install verification token"](https://developer.apple.com/documentation/marketplacekit/supplying-an-install-verification-token)).
The reference implementation is the `app-fair-marketplace-token` crate in this repository:

| Claim | Value |
|---|---|
| header `alg` / `typ` | `ES256` / `JWT` |
| header `kid` | the alternative distribution key's id (`data.id` from App Store Connect) |
| `iss` | **App Fair's** Apple item id (the marketplace app's own) |
| `iat`, `exp` | now, and now + 20 minutes (`DEFAULT_LIFETIME`) |
| `aud` | `AppleDownloadVerification-v1` |
| `bid` | the app's bundle id |
| `dtype` | `download` the first time this **account** gets this app, `redownload` after |
| `nonce` | a fresh UUID |
| `iid`, `vid` | the app's item id and version id, when known |

`dtype` feeds AdAttributionKit, so the server records, per account and app, whether it has issued
a token before; it cannot be derived from the request.

```rust
use app_fair_marketplace_token::{Claims, DownloadType, Signer};

let signer = Signer::from_pem(&private_key_pem, key_id)?;
let claims = Claims::new(APP_FAIR_APPLE_ITEM_ID, &bundle_id, dtype).item(item).version(version);
let token = signer.sign(&claims, unix_now)?;
```

`cargo run -p app-fair-marketplace-token -- sign …` and `… verify …` mint and check a token by hand.

### Packages

`https://packages.appfair.cloud/{appleItemId}/` is the newest available package of that app, and
`…/{appleItemId}/{appleVersionId}/` one version's, laid out as Apple describes
(["Ingesting an alternative distribution package"](https://developer.apple.com/documentation/marketplacekit/ingesting-an-alternative-distribution-package)):
`manifest.json` (byte for byte as App Store Connect delivered it), `signature`,
`variant/<publicId>.ipa`, `delta/<publicId>.ipa`. Nothing is stored there: every file is a release
asset of the app's own GitHub repository, attached by the App Fair Publisher
(`appfair/appfair-apps`, `adp.yml`) as `x-adp-<appleVersionId>-<path with / as ->`, and each request
is answered with a redirect to it (or the bytes streamed through, should iOS refuse a redirect).

A catalog's `adp.url` is used as is when it is on the marketplace domain (`appfair.cloud` or a
subdomain, over HTTPS); a URL anywhere else (a GitHub release, an AltStore PAL host) is replaced by
the canonical one above (`app_fair_marketplace::package_url`). App Store Connect notifies App Fair
of every version of every app whose developer chose it, so every such app has one.

## The endpoints the system calls

These are Apple's, documented in
["Installing apps from an alternative marketplace"](https://developer.apple.com/documentation/marketplacekit/installing-apps-from-an-alternative-marketplace).
App Fair's app never calls them; they are listed so the server is complete.

`https://<marketplace domain>/.well-known/marketplace-kit` (HTTPS with a valid certificate, no
redirects), as the Worker serves it at `https://appfair.cloud/.well-known/marketplace-kit`:

```json
{
  "license": {
    "dynamicLicenseURL": "https://api.appfair.cloud/v1/license",
    "licenseRenewalURL": "https://api.appfair.cloud/v1/license/renew",
    "licenseResolutionURL": "https://appfair.net/marketplace/license-resolution/",
    "signingCertificateURL": "https://appfair.net/marketplace/certs/signing.cer",
    "encryptionCertificateURL": "https://appfair.net/marketplace/certs/encryption.cer"
  },
  "restore": "https://api.appfair.cloud/v1/restore",
  "updates": { "url": "https://api.appfair.cloud/v1/updates", "pollingInterval": 24 }
}
```

- **license**: licenses come from Apple's
  [App License Delivery SDK](https://developer.apple.com/documentation/applicensedeliverysdk)
  (a Swift server library), using the `alternativeDistributionKeyBlob` stored with each variant and
  delta at ingestion. A renewal reissues the license ids the server issued.
- **restore** and **updates**: answer `{"restores": [...]}` / `{"updates": [...]}` with, per app,
  `appleItemId`, `appleVersionId`, `alternativeDistributionPackage` (the package URL above),
  `installVerificationToken` (signed as above, `dtype: redownload`), and `appShareURL`; `updates`
  adds `bundleVersion` and `shortVersionString`. An app no longer offered goes in `failures` with
  `code: 404`.
- Every request carries the header the extension adds, `X-AppFair-Account: <account>`. Nothing
  requires authorization, so the server never answers 401; were one to, the extension's
  `requestFailed` returns `false` (there is no sign-in to renew).

## Ingestion

App Store Connect posts `AlternativeDistributionPackageVersionAvailable` to
`https://api.appfair.cloud/v1/notifications`, signed (`x-apple-signature: hmacsha256=…`) with the
secret registered with the webhook. The Worker:

1. reads `GET /v1/alternativeDistributionPackageVersions/{id}` and the zip it names (manifest.json
   and signature);
2. reads each variant's and delta's `alternativeDistributionKeyBlob` and download URL, and keeps
   the key blobs, sealed, in D1;
3. sends `adp-available` to `appfair/appfair-apps`, whose `adp.yml` attaches the files to the
   release of the repository `marketplace.yaml` names (tag `v<version>` or `<version>`) and calls
   back `POST /v1/published` with a GitHub OIDC token;
4. marks the version available and asks appfair.net to rebuild, whose `aggregate.mjs` adds
   `distribution.adp` and the notarized version to the app's iOS entry in `appindex.v1.json`.

Handle the removal notifications (a developer withdrawing an app, an Apple take-down) by dropping
the app from the catalog and answering `failures` / token refusals for it.

## The catalog

App Fair's iOS default catalog is `https://appfair.net/appindex.v1.json`, an
[App Index](https://appfair.org/schemas/appindex/) document, rebuilt hourly and whenever the
marketplace publishes a version. An iOS app appears in it when its
`platforms.ios` carries `distribution.adp`:

```json
"ios": {
  "version": "1.4.23",
  "buildNumber": "236",
  "bundleIdentifier": "org.appfair.app.Net-Skip",
  "minimumSystemVersion": "17.0",
  "distribution": {
    "adp": {
      "appleItemId": "1640618584",
      "appleVersionId": "869899373",
      "url": "https://packages.appfair.cloud/1640618584/",
      "requiredDeviceCapabilities": ["arm64"]
    }
  }
}
```

`minimumSystemVersion`, `requiredDeviceCapabilities` and `ageRatingRank` feed MarketplaceKit's
own `InstallRequirements` check on the device, so copy them from the package manifest. App Fair
also reads AltStore sources: an app with a `marketplaceID` is offered, with its newest version's
`downloadURL` as the package URL (replaced by the canonical path when it is not on App Fair's
domain). AltStore sources carry no Apple version id, so their apps update through the server's
`updates` endpoint rather than the extension.

## Go-live checklist

When Apple approves App Fair as an alternative app marketplace:

**Apple Developer and App Store Connect**

- [ ] Request the marketplace entitlement for each region (EU; separately Japan and Brazil):
      `com.apple.developer.marketplace.app-installation`. It needs a **new** bundle id; if
      `org.appfair.AppFair` already has App Store history, pick a new one and update
      `Day.toml [app] id`, the App Group, and `DEFAULT_APP_GROUP`.
- [x] Register the App Group `group.org.appfair.AppFair` and enable it for the app and for
      `org.appfair.AppFair.MarketplaceExtension` (done 2026-11-11).
- [x] Development profiles for both bundle ids with the marketplace capability (app) and the App
      Group (both): Xcode's team profiles, which include it (done 2026-11-11). Distribution
      profiles wait on the distribution entitlement.
- [x] Alternative distribution key uploaded (2026-11-12): id
      `b3818104-a7f6-4bd7-8af0-ee63bad3e348`, linked to the App Fair app record (Apple ID
      6480579814). The private key is outside any repository, in
      `appfair/marketplace/certificate/alternative-distribution/`. App Store Connect rejects a PEM
      that ends in a newline ("Invalid public key format"): send it trimmed.
- [x] App Fair Project connected as a developer (2026-11-12): a marketplace token (`iss`
      6480579814, `pid` the team's Developer ID, from `appfair-install-token marketplace`) added in
      Users and Access → Integrations → Marketplace, with Net Skip selected and notifications on.
- [x] Marketplace domain `appfair.cloud` registered (2026-11-12, id
      `c3984703-566f-4632-ace0-1f394fd6912b`). App Store Connect first required a team-level
      alternative distribution key: the same public key, uploaded again without an app link
      (`ba9a5fd2-94f5-413d-803d-06e95c6b49d3`). Domains and keys need an Admin API key.
- [x] Marketplace webhook `https://api.appfair.cloud/v1/notifications` registered (2026-11-12,
      `e4cce735-771f-43d9-82fa-d9a21d1df43d`), with its secret.
- [x] Fleet Reader, Games Fair, and Stanza Redux added to App Fair's marketplace entry beside Net
      Skip (2026-11-12).
- [x] App License Delivery certificates reissued (2026-11-12, valid to 2027-12-12; the 2024 ones
      expired 2025-05-04) from the saved CSRs, so the 2024 keys still apply. Files in
      `appfair/marketplace/certificate/ALD/2026-11-12/` with a new PASK; certificates published
      at `appfair.net/marketplace/certs/`; `ALD_*` secrets set on the Worker. Renew before
      2027-12-12.
- [ ] Optionally upload a Spotlight catalog (Marketplace Search Configurations).

**Server** (`appfair/appfair.cloud`)

- [x] The Worker deployed on `appfair.cloud`, `api.appfair.cloud`, `packages.appfair.cloud`, with
      D1 and its secrets (2026-11-12).
- [ ] The license container deployed (`/v1/license` answers 503 until then).
- [ ] `adp.yml` and `marketplace.yaml` merged in `appfair/appfair-apps`; `aggregate.mjs` and the
      static marketplace pages merged in `appfair/appfair.net`.
- [ ] Offer every other developer distributing through App Fair a marketplace token
      (`appfair-install-token marketplace --pid <their Developer ID>`).

**The app** (already in place; verify on a device in a supported region)

- [ ] `platform/ios/AppFair.entitlements` (marketplace + App Group) and
      `platform/ios/MarketplaceExtension/MarketplaceExtension.entitlements` (App Group) match the
      profiles. Day signs with the file's entitlements, minus whatever a profile does not grant,
      and warns about each one it drops.
- [ ] `MKSellsDigitalGoods` is `false` in `platform/ios/Runner/Info.plist` (App Fair sells
      nothing; selling would require Core Technology Commission reporting).
- [ ] Install, update, Update all, Open, Delete, cancel during download, and an automatic update
      while App Fair is closed all reach the server and complete.
- [ ] The install sheet shows the server's refusal text for an app it does not distribute.
- [ ] The marketplace app is distributed from App Fair's website (the `marketplace-kit://` install
      link on appfair.net), the only way a marketplace itself installs.

## Not yet done

- **Deep links into App Fair.** When the system asks a marketplace to show an app's page
  (`MarketplaceSceneDelegate.scene(_:askedToDisplay:)`, or
  `UIScene.ConnectionOptions.marketplaceDisplayOption` at launch), App Fair should open that app's
  detail page. Day owns the iOS scene delegate, so this needs a hook in day's UIKit host; until
  then the system opens App Fair to its catalog.
- **Age rating exceptions** (iOS 26.1): a parent's approval for an app above the device's maximum
  rating (`requestAgeException`, `currentAgeExceptionRequests()`). The catalog carries
  `ageRatingRank` and the device check uses it; requesting an exception is not offered.
