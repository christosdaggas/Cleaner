use crate::models::ColorScheme;
use gtk4::{gdk, prelude::*};
use libadwaita as adw;

const LIGHT_WINDOW_BG: gdk::RGBA = gdk::RGBA::new(250.0 / 255.0, 250.0 / 255.0, 251.0 / 255.0, 1.0);
const LIGHT_WINDOW_FG: gdk::RGBA = gdk::RGBA::new(0.0, 0.0, 6.0 / 255.0, 0.8);
const LIGHT_VIEW_BG: gdk::RGBA = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
const LIGHT_HEADERBAR_BG: gdk::RGBA = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
const LIGHT_SIDEBAR_BG: gdk::RGBA = gdk::RGBA::new(235.0 / 255.0, 235.0 / 255.0, 237.0 / 255.0, 1.0);
const LIGHT_SECONDARY_SIDEBAR_BG: gdk::RGBA =
    gdk::RGBA::new(243.0 / 255.0, 243.0 / 255.0, 245.0 / 255.0, 1.0);
const LIGHT_CARD_BG: gdk::RGBA = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
const LIGHT_DIALOG_BG: gdk::RGBA = gdk::RGBA::new(250.0 / 255.0, 250.0 / 255.0, 251.0 / 255.0, 1.0);
const LIGHT_POPOVER_BG: gdk::RGBA = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
const DARK_WINDOW_BG: gdk::RGBA = gdk::RGBA::new(34.0 / 255.0, 34.0 / 255.0, 38.0 / 255.0, 1.0);
const DARK_WINDOW_FG: gdk::RGBA = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
const DARK_VIEW_BG: gdk::RGBA = gdk::RGBA::new(29.0 / 255.0, 29.0 / 255.0, 32.0 / 255.0, 1.0);
const DARK_HEADERBAR_BG: gdk::RGBA = gdk::RGBA::new(46.0 / 255.0, 46.0 / 255.0, 50.0 / 255.0, 1.0);
const DARK_SIDEBAR_BG: gdk::RGBA = gdk::RGBA::new(46.0 / 255.0, 46.0 / 255.0, 50.0 / 255.0, 1.0);
const DARK_SECONDARY_SIDEBAR_BG: gdk::RGBA =
    gdk::RGBA::new(40.0 / 255.0, 40.0 / 255.0, 44.0 / 255.0, 1.0);
const DARK_CARD_BG: gdk::RGBA = gdk::RGBA::new(1.0, 1.0, 1.0, 0.08);
const DARK_DIALOG_BG: gdk::RGBA = gdk::RGBA::new(54.0 / 255.0, 54.0 / 255.0, 58.0 / 255.0, 1.0);
const DARK_POPOVER_BG: gdk::RGBA = gdk::RGBA::new(54.0 / 255.0, 54.0 / 255.0, 58.0 / 255.0, 1.0);
const DEFAULT_ACCENT_BG: gdk::RGBA = gdk::RGBA::new(53.0 / 255.0, 132.0 / 255.0, 228.0 / 255.0, 1.0);
const DEFAULT_ACCENT_FG: gdk::RGBA = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ThemeSnapshot {
    pub window_bg: gdk::RGBA,
    pub window_fg: gdk::RGBA,
    pub view_bg: gdk::RGBA,
    pub view_fg: gdk::RGBA,
    pub headerbar_bg: gdk::RGBA,
    pub headerbar_fg: gdk::RGBA,
    pub sidebar_bg: gdk::RGBA,
    pub sidebar_fg: gdk::RGBA,
    pub secondary_sidebar_bg: gdk::RGBA,
    pub secondary_sidebar_fg: gdk::RGBA,
    pub card_bg: gdk::RGBA,
    pub card_fg: gdk::RGBA,
    pub dialog_bg: gdk::RGBA,
    pub dialog_fg: gdk::RGBA,
    pub popover_bg: gdk::RGBA,
    pub popover_fg: gdk::RGBA,
    pub tooltip_bg: gdk::RGBA,
    pub tooltip_fg: gdk::RGBA,
    pub accent_bg: gdk::RGBA,
    pub accent_fg: gdk::RGBA,
    pub accent_standalone: gdk::RGBA,
    pub success: gdk::RGBA,
    pub warning: gdk::RGBA,
    pub error: gdk::RGBA,
    pub destructive: gdk::RGBA,
    pub primary_text: gdk::RGBA,
    pub secondary_text: gdk::RGBA,
    pub border: gdk::RGBA,
    pub divider: gdk::RGBA,
    pub shade: gdk::RGBA,
    pub selection_bg: gdk::RGBA,
    pub selection_fg: gdk::RGBA,
    pub button_bg: gdk::RGBA,
    pub button_fg: gdk::RGBA,
    pub button_border: gdk::RGBA,
    pub flat_button_hover: gdk::RGBA,
    pub flat_button_pressed: gdk::RGBA,
    pub icon_button_fg: gdk::RGBA,
    pub text_button_fg: gdk::RGBA,
    pub link_fg: gdk::RGBA,
    pub entry_bg: gdk::RGBA,
    pub entry_fg: gdk::RGBA,
    pub entry_border: gdk::RGBA,
    pub focus_ring: gdk::RGBA,
    pub active_state: gdk::RGBA,
    pub hover_state: gdk::RGBA,
    pub pressed_state: gdk::RGBA,
    pub overlay_bg: gdk::RGBA,
    pub is_dark: bool,
    pub is_high_contrast: bool,
    pub reduced_motion: bool,
}

impl ThemeSnapshot {
    pub fn from_widget(widget: &impl IsA<gtk4::Widget>) -> Self {
        let widget = widget.as_ref();
        let style_manager = adw::StyleManager::default();
        let is_dark = style_manager.is_dark();
        let is_high_contrast = style_manager.is_high_contrast();
        let reduced_motion = !adw::is_animations_enabled(widget);

        let window_fg = lookup_color(widget, "window_fg_color", fallback_window_fg(is_dark));
        let view_fg = lookup_color(widget, "view_fg_color", fallback_view_fg(is_dark));
        let accent_bg = lookup_color(widget, "accent_bg_color", DEFAULT_ACCENT_BG);
        let error = lookup_color(widget, "error_bg_color", accent_bg);
        let primary_text = view_fg;
        let secondary_text = with_alpha(&view_fg, if is_high_contrast { 0.82 } else { 0.72 });
        let border = lookup_color(widget, "border_color", with_alpha(&window_fg, 0.12));
        let shade = lookup_color(
            widget,
            "shade_color",
            if is_dark {
                gdk::RGBA::new(0.0, 0.0, 0.0, 0.22)
            } else {
                gdk::RGBA::new(0.0, 0.0, 0.0, 0.08)
            },
        );
        let hover_state = with_alpha(&window_fg, if is_high_contrast { 0.14 } else { 0.08 });
        let pressed_state = with_alpha(&window_fg, if is_high_contrast { 0.20 } else { 0.12 });

        Self {
            window_bg: lookup_color(widget, "window_bg_color", fallback_window_bg(is_dark)),
            window_fg,
            view_bg: lookup_color(widget, "view_bg_color", fallback_view_bg(is_dark)),
            view_fg,
            headerbar_bg: lookup_color(widget, "headerbar_bg_color", fallback_headerbar_bg(is_dark)),
            headerbar_fg: lookup_color(widget, "headerbar_fg_color", fallback_headerbar_fg(is_dark)),
            sidebar_bg: lookup_color(widget, "sidebar_bg_color", fallback_sidebar_bg(is_dark)),
            sidebar_fg: lookup_color(widget, "sidebar_fg_color", fallback_sidebar_fg(is_dark)),
            secondary_sidebar_bg: lookup_color(
                widget,
                "secondary_sidebar_bg_color",
                fallback_secondary_sidebar_bg(is_dark),
            ),
            secondary_sidebar_fg: lookup_color(
                widget,
                "secondary_sidebar_fg_color",
                fallback_secondary_sidebar_fg(is_dark),
            ),
            card_bg: lookup_color(widget, "card_bg_color", fallback_card_bg(is_dark)),
            card_fg: lookup_color(widget, "card_fg_color", fallback_card_fg(is_dark)),
            dialog_bg: lookup_color(widget, "dialog_bg_color", fallback_dialog_bg(is_dark)),
            dialog_fg: lookup_color(widget, "dialog_fg_color", fallback_dialog_fg(is_dark)),
            popover_bg: lookup_color(widget, "popover_bg_color", fallback_popover_bg(is_dark)),
            popover_fg: lookup_color(widget, "popover_fg_color", fallback_popover_fg(is_dark)),
            tooltip_bg: lookup_color(widget, "popover_bg_color", fallback_popover_bg(is_dark)),
            tooltip_fg: lookup_color(widget, "popover_fg_color", fallback_popover_fg(is_dark)),
            accent_bg,
            accent_fg: lookup_color(widget, "accent_fg_color", DEFAULT_ACCENT_FG),
            // Custom Cairo drawings cannot inherit the CSS foreground like a
            // symbolic GtkImage. Prefer libadwaita's live accent RGBA when the
            // runtime provides it (1.6+), with the CSS value as a fallback for
            // older supported runtimes.
            accent_standalone: runtime_accent_color(&style_manager)
                .unwrap_or_else(|| accent_standalone_color(&accent_bg)),
            success: lookup_color(widget, "success_bg_color", accent_bg),
            warning: lookup_color(widget, "warning_bg_color", accent_bg),
            error,
            destructive: lookup_color(widget, "destructive_bg_color", error),
            primary_text,
            secondary_text,
            border,
            divider: with_alpha(&border, if is_high_contrast { 1.0 } else { 0.8 }),
            shade,
            selection_bg: with_alpha(&accent_bg, if is_high_contrast { 0.28 } else { 0.18 }),
            selection_fg: lookup_color(widget, "accent_fg_color", DEFAULT_ACCENT_FG),
            button_bg: lookup_color(widget, "card_bg_color", fallback_card_bg(is_dark)),
            button_fg: lookup_color(widget, "card_fg_color", fallback_card_fg(is_dark)),
            button_border: with_alpha(&border, if is_high_contrast { 1.0 } else { 0.85 }),
            flat_button_hover: hover_state,
            flat_button_pressed: pressed_state,
            icon_button_fg: window_fg,
            text_button_fg: accent_bg,
            link_fg: accent_bg,
            entry_bg: lookup_color(widget, "view_bg_color", fallback_view_bg(is_dark)),
            entry_fg: view_fg,
            entry_border: with_alpha(&window_fg, if is_high_contrast { 0.28 } else { 0.14 }),
            focus_ring: with_alpha(&accent_bg, 0.95),
            active_state: with_alpha(&accent_bg, if is_high_contrast { 0.26 } else { 0.16 }),
            hover_state,
            pressed_state,
            overlay_bg: with_alpha(
                &lookup_color(widget, "dialog_bg_color", fallback_dialog_bg(is_dark)),
                if is_dark { 0.92 } else { 0.96 },
            ),
            is_dark,
            is_high_contrast,
            reduced_motion,
        }
    }
}

pub fn apply_color_scheme(style_manager: &adw::StyleManager, scheme: ColorScheme) {
    let color_scheme = match scheme {
        ColorScheme::System => adw::ColorScheme::Default,
        ColorScheme::Light => adw::ColorScheme::ForceLight,
        ColorScheme::Dark => adw::ColorScheme::ForceDark,
    };

    style_manager.set_color_scheme(color_scheme);
}

pub fn sync_runtime_classes(widget: &impl IsA<gtk4::Widget>) {
    let widget = widget.as_ref();
    let style_manager = adw::StyleManager::default();

    set_class(widget, "data-cleaner-dark", style_manager.is_dark());
    set_class(widget, "data-cleaner-light", !style_manager.is_dark());
    set_class(widget, "data-cleaner-high-contrast", style_manager.is_high_contrast());
    set_class(
        widget,
        "data-cleaner-reduced-motion",
        !adw::is_animations_enabled(widget),
    );
}

pub fn transition_duration(widget: &impl IsA<gtk4::Widget>, default_ms: u32) -> u32 {
    if adw::is_animations_enabled(widget) {
        default_ms
    } else {
        0
    }
}

pub fn rgba_to_cairo(color: &gdk::RGBA) -> (f64, f64, f64, f64) {
    (
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    )
}

/// Looks up a named color from the widget's style context.
///
/// **Deprecated API:** `gtk_style_context_lookup_color` was deprecated in GTK 4.10.
/// There is no direct GTK4 replacement for querying arbitrary named CSS colors from a widget.
/// `Widget::color()` only returns the foreground color. Until an equivalent replacement
/// is available in the Rust bindings, this function is retained with the deprecation allowed.
#[allow(deprecated)]
fn lookup_color(widget: &gtk4::Widget, name: &str, fallback: gdk::RGBA) -> gdk::RGBA {
    widget.style_context().lookup_color(name).unwrap_or(fallback)
}

fn with_alpha(color: &gdk::RGBA, alpha: f32) -> gdk::RGBA {
    color.with_alpha(alpha)
}

fn runtime_accent_color(style_manager: &adw::StyleManager) -> Option<gdk::RGBA> {
    style_manager
        .find_property("accent-color-rgba")
        .map(|_| style_manager.property::<gdk::RGBA>("accent-color-rgba"))
}

fn accent_standalone_color(color: &gdk::RGBA) -> gdk::RGBA {
    *color
}

fn set_class(widget: &gtk4::Widget, css_class: &str, enabled: bool) {
    if enabled {
        widget.add_css_class(css_class);
    } else {
        widget.remove_css_class(css_class);
    }
}

const fn fallback_window_bg(is_dark: bool) -> gdk::RGBA {
    if is_dark { DARK_WINDOW_BG } else { LIGHT_WINDOW_BG }
}

const fn fallback_window_fg(is_dark: bool) -> gdk::RGBA {
    if is_dark { DARK_WINDOW_FG } else { LIGHT_WINDOW_FG }
}

const fn fallback_view_bg(is_dark: bool) -> gdk::RGBA {
    if is_dark { DARK_VIEW_BG } else { LIGHT_VIEW_BG }
}

const fn fallback_view_fg(is_dark: bool) -> gdk::RGBA {
    fallback_window_fg(is_dark)
}

const fn fallback_headerbar_bg(is_dark: bool) -> gdk::RGBA {
    if is_dark { DARK_HEADERBAR_BG } else { LIGHT_HEADERBAR_BG }
}

const fn fallback_headerbar_fg(is_dark: bool) -> gdk::RGBA {
    fallback_window_fg(is_dark)
}

const fn fallback_sidebar_bg(is_dark: bool) -> gdk::RGBA {
    if is_dark { DARK_SIDEBAR_BG } else { LIGHT_SIDEBAR_BG }
}

const fn fallback_sidebar_fg(is_dark: bool) -> gdk::RGBA {
    fallback_window_fg(is_dark)
}

const fn fallback_secondary_sidebar_bg(is_dark: bool) -> gdk::RGBA {
    if is_dark { DARK_SECONDARY_SIDEBAR_BG } else { LIGHT_SECONDARY_SIDEBAR_BG }
}

const fn fallback_secondary_sidebar_fg(is_dark: bool) -> gdk::RGBA {
    fallback_window_fg(is_dark)
}

const fn fallback_card_bg(is_dark: bool) -> gdk::RGBA {
    if is_dark { DARK_CARD_BG } else { LIGHT_CARD_BG }
}

const fn fallback_card_fg(is_dark: bool) -> gdk::RGBA {
    fallback_window_fg(is_dark)
}

const fn fallback_dialog_bg(is_dark: bool) -> gdk::RGBA {
    if is_dark { DARK_DIALOG_BG } else { LIGHT_DIALOG_BG }
}

const fn fallback_dialog_fg(is_dark: bool) -> gdk::RGBA {
    fallback_window_fg(is_dark)
}

const fn fallback_popover_bg(is_dark: bool) -> gdk::RGBA {
    if is_dark { DARK_POPOVER_BG } else { LIGHT_POPOVER_BG }
}

const fn fallback_popover_fg(is_dark: bool) -> gdk::RGBA {
    fallback_window_fg(is_dark)
}
