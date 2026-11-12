//! The download/install overlay: a card over a dimmed backdrop with a progress bar while
//! downloading, a spinner while the system installer runs, and a result with a dismiss button.
//! It reads the install signal reactively, so progress and phase update in place.

use day::prelude::*;

use crate::detail::DANGER;
use crate::state::{self, InstallPhase};
use crate::ui::ACCENT;

const SCRIM: Color = Color::rgba(0.0, 0.0, 0.0, 0.5);

fn human(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    }
}

/// The overlay, gated by the caller with `when(install.is_some, …)`.
pub fn install_sheet() -> impl Piece {
    let install = state::install_signal();

    let title = label(move || install.get().map(|u| u.name).unwrap_or_default()).font(Font::Title);

    let card = column((
        title,
        // One body per phase, rebuilt only when the phase changes (progress ticks keep the
        // downloading body and update it in place).
        keyed(
            move || install.get().map(|u| u.phase),
            move |phase| match phase {
                // Downloading: a determinate bar + byte counts + Cancel.
                InstallPhase::Downloading => column((
                    label(crate::res::str::install_downloading())
                        .font(Font::Subheadline)
                        .secondary(),
                    progress(move || install.get().map(|u| u.progress as f64).unwrap_or(0.0))
                        .id(day::id!(install_progress)),
                    label(move || {
                        install
                            .get()
                            .filter(|u| u.total > 1)
                            .map(|u| {
                                crate::res::str::install_bytes(crate::res::str::InstallBytes {
                                    done: human(u.downloaded),
                                    total: human(u.total),
                                })
                                .format()
                            })
                            .unwrap_or_default()
                    })
                    .font(Font::Footnote)
                    .secondary(),
                    button(crate::res::str::btn_cancel())
                        .action(crate::install::cancel)
                        .id(day::id!(install_cancel)),
                ))
                .spacing(10.0)
                .align(HAlign::Leading)
                .any(),
                // iOS: the marketplace server is authorizing the install.
                InstallPhase::Authorizing => column((
                    spinner(),
                    label(crate::res::str::install_authorizing())
                        .font(Font::Subheadline)
                        .secondary(),
                ))
                .spacing(10.0)
                .align(HAlign::Leading)
                .any(),
                // Installing: the system confirm screen is up. On iOS the system finishes the
                // install on its own, so the sheet can be closed while it does.
                InstallPhase::Installing => column((
                    spinner(),
                    label(crate::res::str::install_installing())
                        .font(Font::Subheadline)
                        .secondary(),
                    if crate::market::enabled() {
                        label(crate::res::str::install_confirm_hint_ios())
                    } else {
                        label(crate::res::str::install_confirm_hint())
                    }
                    .font(Font::Footnote)
                    .secondary(),
                    crate::market::enabled()
                        .then(|| dismiss_button(crate::res::str::btn_close(), false)),
                ))
                .spacing(10.0)
                .align(HAlign::Leading)
                .any(),
                // Terminal states.
                InstallPhase::Success => column((
                    label(crate::res::str::install_done())
                        .font(Font::Headline)
                        .color(ACCENT),
                    dismiss_button(crate::res::str::btn_done(), true),
                ))
                .spacing(10.0)
                .align(HAlign::Leading)
                .any(),
                InstallPhase::Cancelled => column((
                    label(crate::res::str::install_cancelled())
                        .font(Font::Headline)
                        .secondary(),
                    dismiss_button(crate::res::str::btn_close(), false),
                ))
                .spacing(10.0)
                .align(HAlign::Leading)
                .any(),
                InstallPhase::Failed(message) => column((
                    label(crate::res::str::install_failed())
                        .font(Font::Headline)
                        .color(DANGER),
                    label(message.clone()).font(Font::Footnote).secondary(),
                    dismiss_button(crate::res::str::btn_close(), false),
                ))
                .spacing(10.0)
                .align(HAlign::Leading)
                .any(),
            },
        ),
    ))
    .spacing(14.0)
    .align(HAlign::Leading)
    .padding(10.0)
    .card()
    .width(320.0);

    // Center the card over a full-bleed dim backdrop.
    card.aligned(Alignment::Center)
        .grow(Dimension::Both)
        .background(SCRIM)
}

fn dismiss_button<M>(text: impl IntoText<M>, prominent: bool) -> impl Piece {
    button(text)
        .apply_if(prominent, |b| b.prominent())
        .action(|| state::set_install(None))
        .id(day::id!(install_dismiss))
}
