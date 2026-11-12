//! The Updates tab: every installed app that App Fair's catalogs know about, with the ones that
//! have a newer version highlighted. Tapping a row opens the detail page (where Update lives).

use std::sync::atomic::{AtomicBool, Ordering};

use day::async_image::ContentMode;
use day::prelude::*;

use crate::model::{AppDetail, AppSummary};
use crate::ui::{ACCENT, PLACEHOLDER};
use crate::{Nav, catalog, push_nav, settings, state};

/// Fires the opt-in auto-update batch at most once per launch, so a re-render doesn't restart it.
static AUTO_FIRED: AtomicBool = AtomicBool::new(false);

/// "Up to date": a green that holds its contrast on light and dark rows.
pub(crate) const OK: Color = Color::hex(0x34_A8_53);

/// One installed app: its catalog summary + the on-device versionCode. `updatable` is derived.
#[derive(Clone, PartialEq)]
struct Installed {
    app: AppSummary,
    installed_code: i64,
}

impl Installed {
    fn updatable(&self) -> bool {
        self.app.version_code > self.installed_code
    }
}

/// The installed apps, updatable first, then alphabetical.
fn sorted(installed: Vec<(AppSummary, i64)>) -> Vec<Installed> {
    let mut v: Vec<Installed> = installed
        .into_iter()
        .map(|(app, installed_code)| Installed {
            app,
            installed_code,
        })
        .collect();
    v.sort_by(|a, b| {
        b.updatable()
            .cmp(&a.updatable())
            .then_with(|| a.app.name.to_lowercase().cmp(&b.app.name.to_lowercase()))
    });
    v
}

/// Install every updatable app in `rows`, one after another (#8), once their full records come
/// back from the catalog.
fn update_all(rows: Vec<Installed>) {
    let wanted: Vec<(u64, String)> = rows
        .into_iter()
        .filter(Installed::updatable)
        .map(|i| (i.app.repo_id, i.app.pkg))
        .collect();
    let details = catalog::read(move |db| {
        let mut out: Vec<AppDetail> = Vec::new();
        for (repo_id, pkg) in wanted {
            out.extend(catalog::detail(db, repo_id, &pkg)?);
        }
        Ok(out)
    });
    day::task(async move {
        match details.await {
            Ok(apps) => state::start_updates(apps),
            Err(e) => error!("update all: {e}"),
        }
    });
}

/// iOS "Update all": a batch MarketplaceKit button over the pending updates, once their full
/// records come back from the catalog.
fn market_update_all(pending: Vec<(u64, String)>) -> AnyPiece {
    let details = state::snapshot(
        Vec::new(),
        || (),
        move |_| {
            let pending = pending.clone();
            catalog::read(move |db| {
                let mut out: Vec<AppDetail> = Vec::new();
                for (repo_id, pkg) in pending {
                    out.extend(catalog::detail(db, repo_id, &pkg)?);
                }
                Ok(out)
            })
        },
    );
    keyed(
        move || {
            let apps = details.get();
            (!apps.is_empty()).then_some(apps)
        },
        |apps| {
            crate::market::update_all_button(apps, crate::res::str::btn_update_all().format())
                .id(day::id!(update_all))
        },
    )
    .any()
}

pub fn updates_page(path: Signal<Vec<Nav>>) -> impl Piece {
    let installed = state::installed_updates();
    let rows = move || sorted(installed.get());

    // When auto-download is on, kick off the batch update once per launch as soon as the catalog
    // reports updates and nothing is already installing (#8).
    watch(
        move || installed.get(),
        move |_, _| {
            let rows = sorted(installed.get_untracked());
            // iOS installs only from a tap on the system's own button, so there is nothing for
            // App Fair to start by itself; the marketplace extension feeds the system's
            // automatic updates instead.
            if !crate::market::enabled()
                && settings::AUTO_DOWNLOAD.get_untracked()
                && !AUTO_FIRED.load(Ordering::Relaxed)
                && !state::install_active()
                && rows.iter().any(Installed::updatable)
            {
                AUTO_FIRED.store(true, Ordering::Relaxed);
                update_all(rows);
            }
        },
    );

    let summary = label(move || {
        let all = rows();
        let n_update = all.iter().filter(|i| i.updatable()).count();
        if all.is_empty() {
            crate::res::str::updates_none().format()
        } else if n_update == 0 {
            crate::res::str::updates_up_to_date(all.len() as i64).format()
        } else {
            crate::res::str::updates_available(n_update as i64).format()
        }
    })
    .font(Font::Footnote)
    .secondary()
    .grow(Dimension::Width)
    .id(day::id!(updates_summary));

    // "Update all" appears only while updates are pending; it installs them one after another.
    // On iOS it is MarketplaceKit's batch button: one tap, one system confirmation, every update.
    let update_all_button = if crate::market::enabled() {
        keyed(
            move || {
                let pending: Vec<(u64, String)> = rows()
                    .into_iter()
                    .filter(Installed::updatable)
                    .map(|i| (i.app.repo_id, i.app.pkg))
                    .collect();
                (!pending.is_empty()).then_some(pending)
            },
            |pending| market_update_all(pending.clone()),
        )
        .any()
    } else {
        when(
            move || rows().iter().any(Installed::updatable),
            move || {
                button(crate::res::str::btn_update_all())
                    .prominent()
                    .action(move || update_all(rows()))
                    .id(day::id!(update_all))
            },
        )
        .any()
    };

    let header = row((summary, update_all_button))
        .align(VAlign::Center)
        .spacing(Space::S);

    let list = list(
        items(rows, |i: &Installed| {
            catalog::app_key(i.app.repo_id, &i.app.pkg)
        }),
        update_row,
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
    .id(day::id!(updates_list))
    .grow(Dimension::Both);

    column((header, list))
        .spacing(10.0)
        .align(HAlign::Leading)
        .padding(Space::M)
}

fn update_row(slot: ItemSlot<Installed, String>) -> impl Piece {
    let icon = crate::icons::row_icon(move || slot.field(|i| i.app.icon_url()))
        .rounded(10.0)
        .content_mode(ContentMode::Fit)
        .placeholder_color(PLACEHOLDER)
        .frame(48.0, 48.0);

    // Right column: an accent "Update →" line when a newer version exists, else "Up to date".
    // Two colored labels toggled reactively (label colors aren't reactive), so a recycled row's
    // status updates on rebind.
    let status = when(
        move || slot.with(|i| i.updatable()),
        move || {
            label(move || {
                slot.with(|i| crate::res::str::status_update(i.app.version_name.clone()).format())
            })
            .font(Font::Caption)
            .color(ACCENT)
        },
    )
    .otherwise(|| {
        label(crate::res::str::status_up_to_date())
            .font(Font::Caption)
            .color(OK)
    });

    row((
        icon,
        column((
            label(move || slot.field(|i| i.app.name.clone()))
                .font(Font::Body)
                .max_lines(1),
            label(move || slot.field(|i| i.app.summary.clone()))
                .font(Font::Footnote)
                .secondary()
                // The rows are a uniform height: two lines of summary is what fits.
                .max_lines(2),
        ))
        .spacing(2.0)
        .align(HAlign::Leading)
        .grow(Dimension::Width),
        status,
    ))
    .spacing(Space::M)
    .align(VAlign::Center)
    .padding(Insets::symmetric(6.0, 8.0))
    .id_of(move || day::id!(update_row, slot.key()))
}
