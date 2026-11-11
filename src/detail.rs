//! The app detail page: icon, name, the install/update/launch action, a device-compatibility
//! badge, a horizontal screenshot filmstrip, the (HTML-flattened) description and "what's new",
//! and the styled, tappable permissions and anti-features.

use day::async_image::{ContentMode, async_image};
use day::prelude::*;
use day::reactive::Resource;

use crate::model::AppDetail;
use crate::ui::{ACCENT, PLACEHOLDER};
use crate::updates::OK;
use crate::{catalog, icons, platform, res, state};

/// Anti-feature amber and incompatibility red, each holding its contrast on light and dark grounds.
const WARN: Color = Color::hex(0xD8_8A_0B);
pub(crate) const DANGER: Color = Color::hex(0xE5_48_4D);
/// The neutral glyph tint (the trailing info mark).
const GLYPH: Color = Color::rgba(0.5, 0.5, 0.55, 1.0);

pub fn detail_page(repo_id: u64, pkg: String) -> impl Piece {
    let detail = Resource::new(
        || (),
        move |_| {
            let pkg = pkg.clone();
            catalog::read(move |db| catalog::detail(db, repo_id, &pkg))
        },
    );
    load(detail, |app: Loaded<Option<AppDetail>>| match app.get() {
        Some(app) => detail_body(&app),
        None => column((label(res::str::app_not_found()).font(Font::Title),))
            .padding(Space::L)
            .any(),
    })
    .loading(|| column((spinner(),)).padding(Space::L))
}

fn detail_body(app: &AppDetail) -> AnyPiece {
    // Compatibility is fixed for a given device + app version, so compute it once. `min_sdk == 0`
    // (repo didn't declare one) or `device_sdk == 0` (unknown/off-device) → don't block.
    let device_sdk = platform::device_sdk();
    let compatible = app.min_sdk == 0 || device_sdk == 0 || app.min_sdk <= device_sdk;
    scroll(
        column((
            header(app, compatible),
            compatibility(app, device_sdk),
            manage(app),
            screenshots(app),
            description(app),
            whats_new(app),
            permissions(app),
            anti_features(app),
            links(app),
        ))
        .spacing(18.0)
        .align(HAlign::Leading)
        .padding(Space::L),
    )
    .any()
}

fn header(app: &AppDetail, compatible: bool) -> impl Piece {
    let icon: AnyPiece = match app.icon_url() {
        Some(url) => async_image(icons::image_signal(&url))
            .rounded(14.0)
            .content_mode(ContentMode::Fit)
            .frame(64.0, 64.0)
            .any(),
        None => rounded_rectangle(14.0)
            .fill(PLACEHOLDER)
            .frame(64.0, 64.0)
            .any(),
    };

    let name = app.name.clone();
    let version = app.version_name.clone();

    // Author line: a link to the author's site when the catalog provides one, else plain text.
    let author_line: AnyPiece = if app.author.is_empty() {
        label(crate::res::str::unknown_author())
            .font(Font::Footnote)
            .secondary()
            .any()
    } else if app.author_website.is_empty() {
        label(app.author.clone())
            .font(Font::Footnote)
            .secondary()
            .any()
    } else {
        link(app.author.clone(), app.author_website.clone())
            .font(Font::Footnote)
            .color(ACCENT)
            .any()
    };

    // "Version 3.4.1 · 11 MB" — append the download size when the catalog reports one.
    let size = format_size(app.apk_size);
    let version_line = if size.is_empty() {
        crate::res::str::version_line(version)
    } else {
        crate::res::str::version_line_size(crate::res::str::VersionLineSize { size, version })
    };

    // Rebuild the action button whenever the catalog changes — a successful install/update bumps
    // it, so the button flips (Install → Installed/Launch) without re-navigating.
    let app_for_button = app.clone();
    let button = keyed(
        move || Some(state::catalog_version().get()),
        move |_| action_button(&app_for_button, compatible),
    );

    row((
        icon,
        // The metadata column takes the width between the icon and the button, so a long title
        // wraps here instead of pushing the action button off the right edge.
        column((
            label(name).font(Font::Headline),
            author_line,
            label(version_line).font(Font::Footnote).secondary(),
        ))
        .spacing(3.0)
        .align(HAlign::Leading)
        .grow(Dimension::Width),
        button,
    ))
    .spacing(14.0)
    .align(VAlign::Center)
}

/// Install / Update / Launch, based on the on-device version and device compatibility.
fn action_button(app: &AppDetail, compatible: bool) -> AnyPiece {
    match platform::installed_version(&app.pkg) {
        // Installed and current → Launch (already on the device, so compatibility is moot).
        Some(v) if v >= app.version_code => launch_button(app),
        Some(_) => install_button(crate::res::str::btn_update(), app, compatible),
        None => install_button(crate::res::str::btn_install(), app, compatible),
    }
}

fn launch_button(app: &AppDetail) -> AnyPiece {
    let (pkg, name) = (app.pkg.clone(), app.name.clone());
    button(crate::res::str::btn_launch())
        .prominent()
        .action(move || {
            let (pkg, name) = (pkg.clone(), name.clone());
            day::task(async move {
                let ok = confirm(crate::res::str::launch_confirm(name.clone()))
                    .confirm_label(crate::res::str::btn_launch())
                    .present()
                    .await;
                if ok {
                    // Report a launch failure instead of silently doing nothing.
                    if let Err(e) = platform::launch_app(&pkg) {
                        alert(crate::res::str::launch_fail_title(name))
                            .message(e)
                            .button(crate::res::str::btn_ok(), ())
                            .present()
                            .await;
                    }
                }
            });
        })
        .id(day::id!(detail_launch))
        .any()
}

/// A prominent Install/Update button when the app can run here; otherwise a secondary button that
/// explains the incompatibility and never starts an install.
fn install_button<M>(verb: impl IntoText<M>, app: &AppDetail, compatible: bool) -> AnyPiece {
    if compatible {
        let app = app.clone();
        return button(verb)
            .prominent()
            .action(move || crate::install::start(&app))
            .id(day::id!(detail_install))
            .any();
    }
    let (name, min_sdk, device_sdk) = (app.name.clone(), app.min_sdk, platform::device_sdk());
    button(verb)
        .action(move || {
            let (name, min_sdk, device_sdk) = (name.clone(), min_sdk, device_sdk);
            day::task(async move {
                alert(crate::res::str::incompatible_title(name))
                    .message(incompatible_message(min_sdk, device_sdk))
                    .button(crate::res::str::btn_ok(), ())
                    .present()
                    .await;
            });
        })
        .id(day::id!(detail_install))
        .any()
}

/// A one-line device-compatibility badge under the header. Hidden when the SDK levels are unknown.
fn compatibility(app: &AppDetail, device_sdk: i64) -> Option<impl Piece> {
    if device_sdk == 0 || app.min_sdk == 0 {
        return None;
    }
    let compatible = app.min_sdk <= device_sdk;
    let (icon, text, color) = if compatible {
        (res::vectors::compatible, res::str::compatible_yes(), OK)
    } else {
        (
            res::vectors::incompatible,
            res::str::compatible_needs(android_release(app.min_sdk)),
            DANGER,
        )
    };
    Some(
        row((
            vector(icon).tint(color).frame(20.0, 20.0),
            label(text)
                .font(Font::Footnote)
                .color(color)
                .grow(Dimension::Width),
        ))
        .spacing(Space::S)
        .align(VAlign::Center),
    )
}

/// Source + uninstall controls, shown only when App Fair installed this app and it's still on the
/// device (#9). Rebuilds on catalog changes so it disappears once the app is uninstalled.
fn manage(app: &AppDetail) -> impl Piece {
    let version = state::catalog_version();
    let pkg = app.pkg.clone();
    // The catalog App Fair installed this app from, by name; `None` when App Fair did not.
    let source = state::snapshot(
        None,
        move || version.get(),
        move |_| {
            let pkg = pkg.clone();
            catalog::read(move |db| match catalog::install_source(db, &pkg)? {
                Some(repo) => Ok(Some(catalog::repo_name(db, repo)?)),
                None => Ok(None),
            })
        },
    );
    let app = app.clone();
    keyed(
        move || Some((source.get(), version.get())),
        move |(source, _)| manage_body(&app, source.clone()),
    )
}

fn manage_body(app: &AppDetail, source: Option<String>) -> Option<impl Piece + use<>> {
    // Only when App Fair has a record of installing this app and it's still on the device.
    let source = source?;
    platform::installed_version(&app.pkg)?;
    let source_text = if source.is_empty() {
        crate::res::str::installed_by_app_fair()
    } else {
        crate::res::str::installed_from(source)
    };

    let (pkg, name) = (app.pkg.clone(), app.name.clone());
    let uninstall = button(crate::res::str::btn_uninstall()).action(move || {
        let (pkg, name) = (pkg.clone(), name.clone());
        day::task(async move {
            let ok = confirm(crate::res::str::uninstall_confirm(name.clone()))
                .confirm_label(crate::res::str::btn_uninstall())
                .present()
                .await;
            if ok {
                match platform::uninstall(&pkg) {
                    Ok(()) => {
                        // Hand ownership back to the user: forget our record and let the button and
                        // this section refresh (the app leaves the device after the system prompt).
                        state::bump_installed();
                        state::write(move |db| catalog::remove_install(db, &pkg));
                    }
                    Err(e) => {
                        alert(crate::res::str::uninstall_fail_title(name))
                            .message(e)
                            .button(crate::res::str::btn_ok(), ())
                            .present()
                            .await;
                    }
                }
            }
        });
    });

    Some(
        row((
            label(source_text)
                .font(Font::Footnote)
                .secondary()
                .grow(Dimension::Width),
            uninstall.id(day::id!(detail_uninstall)),
        ))
        .spacing(Space::M)
        .align(VAlign::Center),
    )
}

/// The blocked-install dialog body.
fn incompatible_message(min_sdk: i64, device_sdk: i64) -> String {
    crate::res::str::incompatible_body(crate::res::str::IncompatibleBody {
        have: android_release(device_sdk),
        need: android_release(min_sdk),
    })
    .format()
}

/// Marketing version + API level for common Android releases, e.g. `24` → `7.0 (API 24)`.
fn android_release(sdk: i64) -> String {
    let name = match sdk {
        21 => "5.0",
        22 => "5.1",
        23 => "6.0",
        24 => "7.0",
        25 => "7.1",
        26 => "8.0",
        27 => "8.1",
        28 => "9",
        29 => "10",
        30 => "11",
        31 => "12",
        32 => "12L",
        33 => "13",
        34 => "14",
        35 => "15",
        36 => "16",
        _ => return format!("API {sdk}"),
    };
    format!("{name} (API {sdk})")
}

/// Human-readable APK size, e.g. `820 KB`, `11 MB`, `1.2 GB`. Empty for a missing/zero size. Uses
/// binary units (1024) like most file managers; one decimal only when it adds information (`55 MB`,
/// not `55.0 MB`). The decimal point isn't locale-adapted — a device locale + CLDR would be needed,
/// which isn't wired up here (the catalog itself is fetched as `en-US`).
fn format_size(bytes: i64) -> String {
    if bytes <= 0 {
        return String::new();
    }
    let b = bytes as f64;
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let (value, unit) = if b >= GB {
        (b / GB, "GB")
    } else if b >= MB {
        (b / MB, "MB")
    } else if b >= KB {
        (b / KB, "KB")
    } else {
        return format!("{bytes} B");
    };
    // Round to one decimal, then drop a trailing `.0`.
    let rounded = (value * 10.0).round() / 10.0;
    if (rounded.fract()).abs() < 0.05 {
        format!("{rounded:.0} {unit}")
    } else {
        format!("{rounded:.1} {unit}")
    }
}

fn screenshots(app: &AppDetail) -> Option<impl Piece> {
    let urls = app.screenshot_urls();
    if urls.is_empty() {
        return None;
    }
    // A horizontal filmstrip of screenshot cards.
    let shots: Vec<_> = urls
        .iter()
        .take(12)
        .map(|url| {
            async_image(icons::image_signal(url))
                .rounded(12.0)
                .content_mode(ContentMode::Fit)
                .frame(190.0, 340.0)
        })
        .collect();
    Some(section_block(
        crate::res::str::section_screenshots(),
        scroll(row(shots).spacing(Space::M).align(VAlign::Top))
            .horizontal()
            .height(340.0),
    ))
}

fn description(app: &AppDetail) -> Option<impl Piece> {
    let raw = if app.description.is_empty() {
        &app.summary
    } else {
        &app.description
    };
    if raw.is_empty() {
        return None;
    }
    // Catalogs put light HTML (<b>, <i>, <a>, lists) in descriptions. Flatten it to plain text.
    // TODO(styled-text): once Day labels support attributed/styled runs, render this as styled
    // text and keep the emphasis and links (see crate::util::strip_html).
    let text = crate::util::strip_html(raw);
    if text.is_empty() {
        return None;
    }
    Some(column((label(text).font(Font::Body),)).align(HAlign::Leading))
}

fn whats_new(app: &AppDetail) -> Option<impl Piece> {
    if app.whats_new.trim().is_empty() {
        return None;
    }
    Some(section_block(
        crate::res::str::section_whats_new(),
        label(crate::util::strip_html(&app.whats_new)).font(Font::Body),
    ))
}

fn permissions(app: &AppDetail) -> Option<impl Piece> {
    if app.permissions.is_empty() {
        return None;
    }
    let rows: Vec<_> = app
        .permissions
        .iter()
        .map(|p| {
            let (title, desc) = platform::permission_info(p);
            let raw = p.clone();
            let dialog_title = title.clone();
            info_row(res::vectors::permission, ACCENT, title, None, move || {
                let (dialog_title, desc, raw) = (dialog_title.clone(), desc.clone(), raw.clone());
                day::task(async move {
                    let sys = crate::res::str::perm_system_label().format();
                    let body = if desc.is_empty() {
                        format!("{sys}\n{raw}")
                    } else {
                        format!("{desc}\n\n{sys}\n{raw}")
                    };
                    alert(dialog_title)
                        .message(body)
                        .button(crate::res::str::btn_ok(), ())
                        .present()
                        .await;
                });
            })
        })
        .collect();
    Some(section_block(
        crate::res::str::section_permissions(),
        column(rows).align(HAlign::Leading).spacing(Space::S),
    ))
}

fn anti_features(app: &AppDetail) -> Option<impl Piece> {
    if app.anti_features.is_empty() {
        return None;
    }
    let rows: Vec<_> = app
        .anti_features
        .iter()
        .map(|af| {
            let (name, desc) = (af.name.clone(), af.description.clone());
            let dialog_title = name.clone();
            info_row(
                res::vectors::anti_feature,
                WARN,
                name,
                Some(WARN),
                move || {
                    let (dialog_title, desc) = (dialog_title.clone(), desc.clone());
                    day::task(async move {
                        let body = if desc.is_empty() {
                            crate::res::str::antifeature_no_desc().format()
                        } else {
                            desc
                        };
                        alert(dialog_title)
                            .message(body)
                            .button(crate::res::str::btn_ok(), ())
                            .present()
                            .await;
                    });
                },
            )
        })
        .collect();
    Some(section_block(
        crate::res::str::section_antifeatures(),
        column(rows).align(HAlign::Leading).spacing(Space::S),
    ))
}

/// One tappable card: a leading category glyph, the title (in `title_color`, else the label
/// color), and a trailing info glyph that signals the row opens a details dialog.
fn info_row(
    icon: VectorName,
    tint: Color,
    title: String,
    title_color: Option<Color>,
    on_tap: impl Fn() + 'static,
) -> impl Piece {
    let title = label(title).font(Font::Body);
    row((
        vector(icon).tint(tint).frame(24.0, 24.0),
        match title_color {
            Some(color) => title.color(color),
            None => title,
        }
        .grow(Dimension::Width),
        vector(res::vectors::info).tint(GLYPH).frame(18.0, 18.0),
    ))
    .spacing(Space::M)
    .align(VAlign::Center)
    .card()
    .on_tap(on_tap)
}

fn links(app: &AppDetail) -> impl Piece {
    column((
        (!app.license.is_empty()).then(|| {
            label(crate::res::str::license_line(app.license.clone()))
                .font(Font::Footnote)
                .secondary()
        }),
        // Website and Source are URLs — render them as links that open the system browser.
        (!app.website.is_empty()).then(|| {
            link(
                crate::res::str::website_line(app.website.clone()),
                app.website.clone(),
            )
            .font(Font::Footnote)
            .color(ACCENT)
        }),
        (!app.source_code.is_empty()).then(|| {
            link(
                crate::res::str::source_line(app.source_code.clone()),
                app.source_code.clone(),
            )
            .font(Font::Footnote)
            .color(ACCENT)
        }),
        label(crate::res::str::package_line(app.pkg.clone()))
            .font(Font::Footnote)
            .secondary(),
        // The pinned signing key (#3): its first bytes are enough for a user to recognize the signer
        // across updates without a wall of hex.
        (!app.signer.is_empty()).then(|| {
            label(crate::res::str::signed_by(short_fingerprint(&app.signer)))
                .font(Font::Footnote)
                .secondary()
        }),
    ))
    .align(HAlign::Leading)
    .spacing(3.0)
}

/// A recognizable prefix of a signing-cert SHA-256, grouped in colon-separated byte pairs, e.g.
/// `43:23:8d:51:2c:1e:5e:b2…`. Full comparison isn't a human task; recognition is.
fn short_fingerprint(fp: &str) -> String {
    let head: String = fp.chars().take(16).collect();
    let grouped = head
        .as_bytes()
        .chunks(2)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(":");
    if fp.chars().count() > 16 {
        format!("{grouped}…")
    } else {
        grouped
    }
}

fn section_block<M>(title: impl IntoText<M>, body: impl Piece) -> impl Piece {
    column((label(title).font(Font::Headline), body))
        .spacing(Space::S)
        .align(HAlign::Leading)
}

#[cfg(test)]
mod tests {
    use super::format_size;

    #[test]
    fn formats_apk_sizes() {
        assert_eq!(format_size(0), "");
        assert_eq!(format_size(-5), "");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2 KB");
        assert_eq!(format_size(11 * 1024 * 1024), "11 MB");
        // 1.5 MiB keeps its decimal; a whole number drops it.
        assert_eq!(format_size(1024 * 1024 + 512 * 1024), "1.5 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1 GB");
        assert_eq!(
            format_size(1024 * 1024 * 1024 + 200 * 1024 * 1024),
            "1.2 GB"
        );
    }
}
