//! Live theme customization.
//!
//! GPUI Kit ships dark/light themes; LocalForge adds an accent color, corner
//! radius and base font size on top so the hub can match the user's desktop
//! without shipping a full theme file.

use gpui_kit::component::theme::Theme;
use gpui_kit::*;
use shared::UiConfig;

/// Applies the saved appearance. Safe to call before the window exists.
pub fn apply(cx: &mut App, ui: &UiConfig, window: Option<&mut Window>) {
    let mode = match ui.theme.as_str() {
        "light" => gpui_kit::component::theme::ThemeMode::Light,
        "system" => gpui_kit::component::theme::ThemeMode::from(cx.window_appearance()),
        _ => gpui_kit::component::theme::ThemeMode::Dark,
    };

    Theme::change(mode, window, cx);

    let accent = parse_hex(&ui.accent_hex()).unwrap_or_else(|| Hsla::from(rgb(0x7c5cff)));
    let radius = ui.radius.clamp(0.0, 20.0);
    let font_size = ui.font_size.clamp(11.0, 20.0);

    Theme::update(cx, |theme| {
        theme.colors.primary = accent;
        theme.colors.primary_hover = Hsla {
            l: (accent.l + 0.07).min(1.0),
            ..accent
        };
        theme.colors.primary_active = Hsla {
            l: (accent.l - 0.07).max(0.0),
            ..accent
        };
        theme.colors.primary_foreground = if accent.l > 0.62 {
            Hsla::from(rgb(0x111111))
        } else {
            Hsla::from(rgb(0xffffff))
        };
        theme.colors.ring = accent;
        theme.colors.selection = Hsla { a: 0.28, ..accent };
        theme.colors.button_primary = accent;
        theme.colors.button_primary_hover = theme.colors.primary_hover;
        theme.colors.button_primary_active = theme.colors.primary_active;
        theme.colors.button_primary_foreground = theme.colors.primary_foreground;

        theme.radius = px(radius);
        theme.radius_lg = px((radius + 4.0).min(24.0));
        theme.font_size = px(font_size);
    });
    cx.refresh_windows();
}

/// Parses `#rgb` or `#rrggbb` into an [`Hsla`] color.
///
/// Returns `None` for any other format, so callers can fall back to a
/// semantic theme token instead of rendering an invisible color.
pub fn parse_hex(value: &str) -> Option<Hsla> {
    let hex = value.trim().trim_start_matches('#');

    let expanded = match hex.len() {
        3 => hex
            .chars()
            .flat_map(|character| [character, character])
            .collect::<String>(),
        6 => hex.to_owned(),
        _ => return None,
    };

    let parsed = u32::from_str_radix(&expanded, 16).ok()?;
    Some(Hsla::from(rgb(parsed)))
}
