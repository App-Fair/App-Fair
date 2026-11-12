# app-fair-marketplace

App Fair's bridge to Apple's MarketplaceKit, the framework an iOS alternative app marketplace
installs, updates, opens and deletes apps through.

MarketplaceKit is Swift-only: its Objective-C header is empty, so `objc2` cannot reach it. This
crate declares the API in Rust with a [daybridge](https://daybrite.dev/docs/reference/bridge)
block (`src/lib.rs`), and `platform/ios/MarketplaceBridge.swift` implements it:

| Rust | What it does on iOS |
|---|---|
| `configure` | the marketplace server and the App Group shared with the extension |
| `account` | the device's anonymous account id (keychain-backed) |
| `status` | who distributed App Fair, the device's catalog region, its maximum age rating |
| `observe` | `AppLibrary` snapshots (installed and installing apps, progress, errors) and install-button events |
| `satisfies` | MarketplaceKit's own `InstallRequirements` check for a catalog entry |
| `cancel` | cancel an installation in progress |
| `share` | write `marketplace.json` for the marketplace extension |
| `action_button` | MarketplaceKit's `ActionButton` (install, batch install, launch, delete) as a Day piece |

The install control has to be MarketplaceKit's `ActionButton`: the system accepts an install only
from a tap on it. The button is hosted through day-piece-swiftui's provider contract
(`DayView_appfair_marketplace_action`). When the person taps it, it asks the marketplace server for
install verification tokens (see `../docs/marketplace-server.md`).

Everywhere but iOS, every call answers `Error::Unsupported`, so the crate builds and its tests run
under bare `cargo test`.
