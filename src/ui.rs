//! The Catalogs tab: a catalog switcher, an "add catalog" button, native search, a category
//! filter, the sync status line, and the app list with icons. Tapping a row pushes the detail
//! page onto the tab's navigation stack.

use day::async_image::ContentMode;
use day::prelude::*;
use day::searchfield::search_field;

use crate::model::{AppSummary, Category, RepoRow, SortOrder};
use crate::state::{self, SyncUi};
use crate::{Nav, catalog, push_nav, sync};

/// The empty icon square while an icon loads: a mid grey that reads on light and dark rows.
pub(crate) const PLACEHOLDER: Color = Color::rgba(0.5, 0.5, 0.55, 0.18);
/// App Fair's blue, which holds its contrast on light and dark grounds.
pub(crate) const ACCENT_HEX: u32 = 0x3D_7B_F0;
pub(crate) const ACCENT: Color = Color::hex(ACCENT_HEX);

pub fn catalogs_page(path: Signal<Vec<Nav>>) -> impl Piece {
    let query = state::query();
    let category = state::category();
    let sort = state::sort_order();
    let repos = state::repos();
    let categories = state::categories();
    let apps = state::search_results();

    let switcher = keyed(move || Some(repos.get()), |repos| catalog_switcher(repos));
    // Adding a custom F-Droid catalog is still a beta feature — the "Add" button is exposed only in
    // debug builds; release builds hide it and browse the default App Fair catalog alone.
    let header = row((
        switcher.grow(Dimension::Width),
        cfg!(debug_assertions).then(|| {
            button(crate::res::str::btn_add())
                .action(move || push_nav(path, Nav::AddCatalog))
                .id(day::id!(catalog_add))
        }),
        button(crate::res::str::btn_refresh())
            .action(sync::sync_all_enabled)
            .id(day::id!(refresh)),
    ))
    .align(VAlign::Center)
    .spacing(Space::S);

    let search = search_field(query)
        .placeholder(crate::res::str::search_placeholder())
        .id(day::id!(search));

    let category_region = keyed(
        move || Some(categories.get()),
        move |cats| category_picker(cats, category),
    );
    // The category filter (which grows to fill) and the sort-order drop-down share one row.
    let controls = row((category_region.grow(Dimension::Width), sort_picker(sort)))
        .spacing(Space::S)
        .align(VAlign::Center);

    let sync = state::sync_signal();
    let sync_line = label(move || sync_text(&sync.get()))
        .font(Font::Footnote)
        .secondary()
        .id(day::id!(sync_status));

    let list = list(
        items(
            move || apps.get(),
            |a: &AppSummary| catalog::app_key(a.repo_id, &a.pkg),
        ),
        app_row,
    )
    .row_height(RowHeight::Uniform(72.0))
    .on_select(move |key: String| {
        if let Some((repo_id, pkg)) = catalog::parse_app_key(&key) {
            push_nav(
                path,
                Nav::App {
                    repo_id,
                    pkg: pkg.to_string(),
                },
            );
        }
    })
    .id(day::id!(app_list))
    .grow(Dimension::Both);

    column((header, search, controls, sync_line, list))
        .spacing(10.0)
        .align(HAlign::Leading)
        .padding(Space::M)
}

/// A native dropdown choosing the catalog list's sort order: Default (the catalog's `rank`), Last
/// Updated, or Name (locale-collated). Selecting one re-queries the list via [`state::search_apps`].
fn sort_picker(sort: Signal<SortOrder>) -> impl Piece {
    picker(
        sort,
        [
            (SortOrder::Default, crate::res::str::sort_default()),
            (SortOrder::LastUpdated, crate::res::str::sort_last_updated()),
            (SortOrder::Name, crate::res::str::sort_name()),
        ],
    )
    .menu()
    .id(day::id!(sort_picker))
}

/// A native dropdown of "All Catalogs" + each enabled catalog. Selecting one filters the list.
fn catalog_switcher(repos: &[RepoRow]) -> impl Piece + use<> {
    let enabled: Vec<_> = repos.iter().filter(|r| r.enabled).collect();

    let options: Vec<(Option<u64>, LocalizedText)> =
        std::iter::once((None, crate::res::str::all_catalogs()))
            .chain(enabled.iter().map(|r| {
                let name = if r.name.is_empty() {
                    r.address.clone()
                } else {
                    r.name.clone()
                };
                let title = crate::res::str::catalog_option(crate::res::str::CatalogOption {
                    count: r.app_count,
                    name,
                });
                (Some(r.id), title)
            }))
            .collect();

    picker(state::repo_filter(), options)
        .menu()
        .id(day::id!(catalog_switcher))
}

/// A native dropdown of "All" + every category. Selecting one filters the list. Nothing until the
/// catalog has categories.
fn category_picker(cats: &[Category], category: Signal<Option<String>>) -> impl Piece + use<> {
    (!cats.is_empty()).then(|| {
        // Category names come from the catalog as plain strings, so the "All" entry joins them
        // as one.
        let options: Vec<(Option<String>, String)> =
            std::iter::once((None, crate::res::str::all_categories().format()))
                .chain(cats.iter().map(|c| (Some(c.key.clone()), c.name.clone())))
                .collect();
        picker(category, options)
            .menu()
            .id(day::id!(category_picker))
    })
}

/// A recycling list row. Every field is read INSIDE a reactive closure, so a recycled cell
/// updates when it rebinds to a new app.
pub(crate) fn app_row(slot: ItemSlot<AppSummary, String>) -> impl Piece {
    let icon = crate::icons::row_icon(move || slot.field(|a| a.icon_url()))
        .rounded(10.0)
        .content_mode(ContentMode::Fit)
        .placeholder_color(PLACEHOLDER)
        .frame(48.0, 48.0);

    row((
        icon,
        column((
            label(move || slot.field(|a| a.name.clone()))
                .font(Font::Body)
                .max_lines(1),
            label(move || slot.field(|a| a.summary.clone()))
                .font(Font::Footnote)
                .secondary()
                // The rows are a uniform height: two lines of summary is what fits.
                .max_lines(2),
        ))
        .spacing(2.0)
        .align(HAlign::Leading)
        .grow(Dimension::Width),
        label(move || slot.field(|a| a.version_name.clone()))
            .font(Font::Caption)
            .color(ACCENT),
    ))
    .spacing(Space::M)
    .align(VAlign::Center)
    .padding(Insets::symmetric(6.0, 8.0))
    .id_of(move || day::id!(app_row, slot.key()))
}

fn sync_text(sync: &SyncUi) -> String {
    match sync {
        SyncUi::Idle => String::new(),
        SyncUi::Checking => crate::res::str::sync_checking().format(),
        SyncUi::Downloading(Some(f)) => {
            crate::res::str::sync_downloading_pct((f * 100.0) as i64).format()
        }
        SyncUi::Downloading(None) => crate::res::str::sync_downloading().format(),
        SyncUi::Building => crate::res::str::sync_building().format(),
        SyncUi::UpToDate => crate::res::str::sync_up_to_date().format(),
        SyncUi::Done(0) => crate::res::str::sync_up_to_date().format(),
        SyncUi::Done(n) => crate::res::str::sync_updated(*n as i64).format(),
        SyncUi::Error(e) => crate::res::str::sync_failed(e.as_str()).format(),
    }
}
