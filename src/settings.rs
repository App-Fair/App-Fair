//! Settings: the catalogs App Fair syncs, when it checks for updates, the anti-feature filters,
//! the image cache, appearance and language, and About. Native settings pages from
//! day-piece-settings over typed preferences (https://daybrite.dev/docs/how-to/settings).

use day::prelude::*;
use day_piece_settings::prelude::*;

use crate::model::RepoRow;
use crate::{catalog, res, state};

/// Whether App Fair checks its catalogs for updates each time it opens.
pub static SYNC_ON_LAUNCH: Pref<bool> = Pref::new("app_fair.sync_on_launch", || true);

/// Whether App Fair downloads available updates by itself; each install still asks (#8).
pub static AUTO_DOWNLOAD: Pref<bool> = Pref::new("app_fair.auto_download", || false);

/// The anti-features a user can hide from browsing and search, each with its preference (#13).
static HIDE: [(&str, Pref<bool>); 9] = [
    ("Ads", Pref::new("app_fair.hide.ads", || false)),
    ("Tracking", Pref::new("app_fair.hide.tracking", || false)),
    (
        "NonFreeNet",
        Pref::new("app_fair.hide.non_free_net", || false),
    ),
    (
        "NonFreeAdd",
        Pref::new("app_fair.hide.non_free_add", || false),
    ),
    (
        "NonFreeDep",
        Pref::new("app_fair.hide.non_free_dep", || false),
    ),
    (
        "UpstreamNonFree",
        Pref::new("app_fair.hide.upstream_non_free", || false),
    ),
    (
        "NonFreeAssets",
        Pref::new("app_fair.hide.non_free_assets", || false),
    ),
    ("KnownVuln", Pref::new("app_fair.hide.known_vuln", || false)),
    (
        "NoSourceSince",
        Pref::new("app_fair.hide.no_source_since", || false),
    ),
];

const PRIVACY_URL: &str = "https://github.com/marcprux/App-Fair/blob/main/PRIVACY.md";
const SOURCE_URL: &str = "https://github.com/marcprux/App-Fair";

/// The anti-feature keys the user chose to hide. A tracked read: browsing re-queries when a
/// filter changes.
pub fn excluded_anti_features() -> Vec<String> {
    HIDE.iter()
        .filter(|(_, pref)| pref.get())
        .map(|(key, _)| key.to_string())
        .collect()
}

/// The localized label of a filterable anti-feature (#16).
fn anti_label(key: &str) -> LocalizedText {
    match key {
        "Ads" => res::str::af_ads(),
        "Tracking" => res::str::af_tracking(),
        "NonFreeNet" => res::str::af_nonfreenet(),
        "NonFreeAdd" => res::str::af_nonfreeadd(),
        "NonFreeDep" => res::str::af_nonfreedep(),
        "UpstreamNonFree" => res::str::af_upstreamnonfree(),
        "NonFreeAssets" => res::str::af_nonfreeassets(),
        "KnownVuln" => res::str::af_knownvuln(),
        _ => res::str::af_nosourcesince(),
    }
}

/// The Settings declaration, installed once in `root()`.
pub fn preferences() -> Prefs {
    let cleared = Signal::new(false);
    let mut updates = Group::new("updates", res::str::settings_updates())
        .icon(res::vectors::tab_updates)
        .row(
            rows::toggle(&SYNC_ON_LAUNCH, res::str::pref_sync_title())
                .caption(res::str::pref_sync_blurb())
                .id(day::id!(pref_sync)),
        );
    // iOS installs only from a tap on the system's button; automatic updates there are the
    // system's own, fed by the marketplace extension.
    if !crate::market::enabled() {
        updates = updates.row(
            rows::toggle(&AUTO_DOWNLOAD, res::str::pref_auto_title())
                .caption(res::str::pref_auto_blurb())
                .id(day::id!(pref_auto_download)),
        );
    }
    // On iOS each install is also authorized by App Fair's marketplace server, which the privacy
    // note has to say.
    let privacy = if crate::market::enabled() {
        res::str::about_privacy_ios()
    } else {
        res::str::about_privacy()
    };
    Preferences::new()
        .title(res::str::tab_settings())
        .about(
            About::from_manifest()
                .tagline(res::str::about_blurb())
                .credits(privacy)
                .source(SOURCE_URL)
                .privacy(PRIVACY_URL),
        )
        .group(
            Group::new("catalogs", res::str::settings_catalogs())
                .icon(res::vectors::tab_catalogs)
                .section(
                    Section::new()
                        .footer(res::str::settings_catalogs_blurb())
                        .row(rows::custom(repo_list)),
                ),
        )
        .group(updates)
        .group(
            Group::new("filters", res::str::settings_hide())
                .icon(res::vectors::anti_feature)
                .section(Section::new().footer(res::str::settings_hide_blurb()).rows(
                    HIDE.iter().map(|(key, pref)| {
                        rows::toggle(pref, anti_label(key)).id(day::id!(anti, *key))
                    }),
                )),
        )
        .group(
            Group::new("storage", res::str::settings_storage())
                .icon(res::vectors::storage)
                .row(
                    rows::action(res::str::clear_cache_title(), move || {
                        let _ =
                            std::fs::remove_dir_all(crate::platform::cache_dir().join("imgcache"));
                        cleared.set(true);
                    })
                    .caption(move || {
                        if cleared.get() {
                            res::str::clear_cache_done().format()
                        } else {
                            res::str::clear_cache_blurb().format()
                        }
                    })
                    .id(day::id!(clear_cache)),
                ),
        )
        .group(general_group(res::locales::ALL))
        .install()
}

/// The catalogs, each with its switch and a Remove button.
fn repo_list() -> impl Piece {
    let repos = state::repos();
    column((each(
        items(move || repos.get(), |r: &RepoRow| r.id),
        repo_row,
    ),))
    .spacing(Space::S)
    .align(HAlign::Leading)
}

fn repo_row(slot: ItemSlot<RepoRow, u64>) -> impl Piece {
    let r = slot.get();
    let id = r.id;
    let address = r.address.clone();

    let enabled = Signal::new(r.enabled);
    watch(
        move || enabled.get(),
        move |on: &bool, _| {
            let on = *on;
            state::write(move |db| catalog::set_repo_enabled(db, id, on));
        },
    );

    row((
        column((
            // Read name and count reactively: the row is keyed by catalog id, so `each` keeps
            // it when a sync fills in the catalog's name and app count.
            label(move || {
                slot.field(|r| {
                    if r.name.is_empty() {
                        r.address.clone()
                    } else {
                        r.name.clone()
                    }
                })
            })
            .font(Font::Body),
            label(move || {
                res::str::repo_row_meta(res::str::RepoRowMeta {
                    address: slot.field(|r| r.address.clone()),
                    count: slot.field(|r| r.app_count),
                })
                .format()
            })
            .font(Font::Caption)
            .secondary(),
        ))
        .spacing(2.0)
        .align(HAlign::Leading)
        .grow(Dimension::Width),
        toggle(enabled).id(day::id!(repo_enabled, id)),
        button(res::str::btn_remove())
            .action(move || {
                crate::cache::remove_index(&address);
                state::write(move |db| catalog::delete_repo(db, id));
            })
            .id(day::id!(repo_remove, id)),
    ))
    .spacing(10.0)
    .align(VAlign::Center)
}
