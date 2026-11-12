# App Fair — Privacy Policy

_Last updated: 2026-11-11_

App Fair is an open app store for App Index, AltStore, and F-Droid catalogs. It is designed to
collect nothing about you.

## What App Fair collects

**Nothing.** App Fair has no accounts, no sign-in, no analytics, no advertising, and no crash or
usage reporting. We do not collect, transmit, or sell any personal or device data. The one
exception is on iOS, where installs carry an anonymous identifier, as described under
[Installing apps on iOS](#installing-apps-on-ios).

## What leaves your device

App Fair connects only to the catalog repositories you choose to add (F-Droid's official
repository is included by default). When you open the app, browse, or install, App Fair requests:

- the catalog **index** (the list of apps and their metadata) from each enabled catalog;
- **icons and screenshots** referenced by that catalog;
- the **app package (APK)** you choose to install or update, from that catalog.

These requests go directly from your device to the catalog's servers over HTTPS. The catalog
operator may see standard request information (such as your IP address and the file requested),
exactly as your web browser would reveal when visiting their site. App Fair adds no identifiers to
these requests and sends nothing about what you browse or install anywhere else.

## What stays on your device

The synced catalog, your list of catalogs, your preferences, and a record of what you installed
through App Fair are stored **only** on your device. Removing the app removes this data.

## Installing apps

App Fair downloads an app's package from its catalog, verifies it against the catalog's published
checksum, and hands it to Android's standard package installer, which asks you to confirm every
installation. App Fair never installs anything silently.

## Installing apps on iOS

On iOS, App Fair is an alternative app marketplace and Apple's system installs every app. When you
tap Install, Update, or Update all, App Fair asks **App Fair's marketplace server** to authorize the
install, and iOS then downloads the app from that server. Those requests carry an **anonymous
identifier** that App Fair creates on your device the first time it runs: a random value, kept in
the device's keychain so it survives reinstalling App Fair, never synced to your other devices, and
never linked to your name, email, or Apple Account. The server uses it only to tell a first
download from a later one, as Apple requires every marketplace to report. iOS's own automatic
updates send the same identifier to the same server.

## Permissions

- **Internet** — to fetch catalog indexes, images, and app packages from the catalogs you add.
- **Install packages** (`REQUEST_INSTALL_PACKAGES`) — to hand a downloaded package to Android's
  installer for your confirmation.
- **Query installed packages** (`QUERY_ALL_PACKAGES`) — to tell whether a catalog app is already
  installed and whether an update is available, so it can show "Install" versus "Update".

## Children

App Fair is not directed to children and does not knowingly collect any data from anyone.

## Contact

Questions about this policy: <privacy@appfair.org>.

---

> **Note for release:** Google Play requires a privacy policy reachable at a public **URL** (entered
> in Play Console → App content → Privacy policy). Publish this text at a stable URL (for example
> `https://appfair.org/privacy`) and enter that URL in the console. Update the contact address above
> to a real, monitored inbox before submitting.
