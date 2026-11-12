# App Fair

App Fair is an open app store for free and open source software. It is written in Rust with [Day](https://daybrite.dev).

It reads three catalog formats: [App Index](https://appfair.org/schemas/appindex/) (its own, and
the primary one), F-Droid's Index V2, and AltStore sources.

- **Android**: it downloads, verifies, installs and updates APKs through Android's system
  installer.
- **iOS**: it is an alternative app marketplace (the EU, Japan, and Brazil). It installs, updates,
  opens and deletes apps through Apple's MarketplaceKit (`marketplace/`, `src/market.rs`), with a
  marketplace extension for automatic updates (`platform/ios/MarketplaceExtension/`). The server
  side and the steps for going live are in [docs/marketplace-server.md](docs/marketplace-server.md).

## Build and run

```sh
cargo install day-cli
day launch -t android-material
day launch -t ios-uikit
```

`APP_FAIR_MOCK=1` runs against a bundled offline catalog, and `APP_FAIR_MARKETPLACE=https://…`
points an iOS build at a staging marketplace server.
