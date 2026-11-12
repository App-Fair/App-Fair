# App Fair — UI strings (https://daybrite.dev/docs/localization). Add a locale with
# `day localize add <locale>`, which writes a sibling folder such as locales/fr/app.ftl.

app_title = App Fair

# Tabs + navigation titles
tab_catalogs = Catalogs
tab_updates = Updates
tab_settings = Settings
nav_updates = Updates
nav_add_catalog = Add catalog

# Common buttons
btn_ok = OK
btn_cancel = Cancel
btn_close = Close
btn_done = Done

# Catalogs tab
btn_add = Add
btn_refresh = Refresh
search_placeholder = Search apps
all_catalogs = All Catalogs
all_categories = All Categories
catalog_option = { $name } ({ $count })

# Sort order drop-down
sort_default = Default
sort_last_updated = Last Updated
sort_name = Name

# Sync status line
sync_checking = Checking for updates…
sync_downloading_pct = Downloading catalog… { $pct }%
sync_downloading = Downloading catalog…
sync_building = Updating catalog…
sync_up_to_date = Catalog up to date
sync_updated = { $count ->
    [one] Updated { $count } app
   *[other] Updated { $count } apps
}
sync_failed = Sync failed: { $error }

# Updates tab
updates_none = No installed apps from your catalogs.
updates_up_to_date = { $count } installed · all up to date
updates_available = { $count ->
    [one] { $count } update available
   *[other] { $count } updates available
}
btn_update_all = Update all
status_up_to_date = Up to date
status_update = Update → { $version }

# Detail page
btn_install = Install
btn_update = Update
btn_launch = Launch
btn_uninstall = Uninstall
btn_open = Open
btn_delete = Delete
app_not_found = App not found
unknown_author = Unknown author
version_line = Version { $version }
version_line_size = Version { $version } · { $size }
compatible_yes = Compatible with your device
compatible_needs = Needs Android { $release } — newer than this device
incompatible_title = { $name } isn't compatible
incompatible_body = This app needs Android { $need }. Your device runs Android { $have }, so it can't be installed.
compatible_needs_ios = Needs iOS { $release } or hardware this device lacks
incompatible_body_ios = This app needs iOS { $need } or later, or hardware this device doesn't have, so it can't be installed here.
market_region = Apple doesn't allow alternative app marketplaces in your region, so App Fair can't install apps on this device.
section_screenshots = Screenshots
section_whats_new = What's new
section_permissions = Permissions
section_antifeatures = Anti-features
license_line = License: { $license }
website_line = Website: { $url }
source_line = Source: { $url }
package_line = Package: { $pkg }
signed_by = Signed by { $fp }
installed_from = Installed from { $source }
installed_by_app_fair = Installed by App Fair
launch_confirm = Launch { $name }?
launch_fail_title = Couldn't launch { $name }
uninstall_confirm = Uninstall { $name }?
uninstall_fail_title = Couldn't uninstall { $name }
perm_system_label = System permission:
antifeature_no_desc = This catalog didn't include a description for this anti-feature.

# Install overlay
install_downloading = Downloading…
install_bytes = { $done } / { $total }
install_installing = Installing…
install_confirm_hint = Confirm the install when Android asks.
install_authorizing = Authorizing the install…
install_confirm_hint_ios = Confirm the install when iOS asks.
install_done = Installed
install_cancelled = Cancelled
install_failed = Install failed

# Settings tab
settings_catalogs = Catalogs
settings_catalogs_blurb = Enable, disable, or remove your catalogs.
settings_updates = Updates
pref_sync_title = Check for updates on launch
pref_sync_blurb = Sync catalogs each time App Fair opens.
pref_auto_title = Download updates automatically
pref_auto_blurb = Fetch available updates in the background; each install still asks first.
clear_cache_title = Clear image cache
clear_cache_blurb = Remove cached icons and screenshots.
clear_cache_done = Cleared.
btn_remove = Remove
settings_hide = Hide apps with…
settings_storage = Storage
settings_hide_blurb = Leave out apps that declare these anti-features when you browse and search.
about_blurb = An open app store for App Index, AltStore, and F-Droid catalogs, built with Day.
about_privacy = App Fair talks only to the catalogs you add. There are no accounts and no analytics — update checks and downloads go straight to those repositories, and nothing about what you browse or install leaves your device.
about_privacy_ios = App Fair talks only to the catalogs you add and, when you install or update an app, to App Fair's marketplace server, which authorizes each install for iOS. That server sees an anonymous identifier made on this device, never your name or Apple Account. There are no accounts and no analytics.
repo_row_meta = { $address }  ·  { $count } apps

# Anti-feature filter labels
af_ads = Advertising
af_tracking = Tracks you
af_nonfreenet = Non-free network services
af_nonfreeadd = Non-free add-ons
af_nonfreedep = Non-free dependencies
af_upstreamnonfree = Non-free upstream
af_nonfreeassets = Non-free assets
af_knownvuln = Known vulnerability
af_nosourcesince = No source since

# Add-catalog page
add_title = Add a catalog
add_blurb = Enter the address of an App Index (appindex.json), an AltStore source, or an F-Droid repository (with its ?fingerprint=), or pick a known repository.
add_placeholder = https://example.org/appindex.json
add_error_insecure = Catalog URLs must use https:// (or a .onion address).
btn_add_catalog = Add catalog
add_known = Known repositories
