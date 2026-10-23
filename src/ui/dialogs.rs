//! Shared helpers for transient windows.
//!
//! `AdwMessageDialog` sizes its body label to its full natural height. A long
//! cleanup summary (for example the output of `journalctl --vacuum-time`) can
//! therefore produce a dialog thousands of pixels tall, placing the response
//! buttons outside the screen. These helpers keep dialog bodies scrollable and
//! bounded by the application window, and apply the same runtime theme classes
//! to every popup so transient surfaces match the main content pane.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

/// Height of a dialog body is capped relative to the parent window. The
/// reserve covers the heading, response buttons, and dialog padding.
const DIALOG_VERTICAL_RESERVE: i32 = 260;
const MIN_BODY_HEIGHT: i32 = 120;
const MAX_BODY_HEIGHT: i32 = 480;
const MIN_BODY_WIDTH: i32 = 360;

/// Mark a window as a Data Cleaner popup and apply the current light/dark,
/// contrast, and motion classes to it.
pub fn prepare_window(window: &impl IsA<gtk4::Window>) {
    let window = window.as_ref();
    window.add_css_class("data-cleaner-dialog");
    crate::theme::sync_runtime_classes(window);
}

/// Maximum height for a scrollable dialog body, derived from the application
/// window so the dialog never grows taller than the window it belongs to.
pub fn body_max_height(parent: Option<&gtk4::Window>) -> i32 {
    let parent_height = parent
        .map(|window| window.height())
        .filter(|height| *height > 0)
        .unwrap_or(720);

    (parent_height - DIALOG_VERTICAL_RESERVE).clamp(MIN_BODY_HEIGHT, MAX_BODY_HEIGHT)
}

/// Wrap a widget in a vertically scrolling region bounded by the parent
/// window's height.
pub fn scrollable_content(
    parent: Option<&gtk4::Window>,
    content: &impl IsA<gtk4::Widget>,
) -> gtk4::ScrolledWindow {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .propagate_natural_height(true)
        .max_content_height(body_max_height(parent))
        .min_content_width(MIN_BODY_WIDTH)
        .child(content)
        .build();
    scrolled.add_css_class("dialog-scroll");
    // Keep the vertical scrollbar in its own gutter instead of fading it as an
    // overlay indicator, so a long dialog body is always visibly scrollable.
    scrolled.set_overlay_scrolling(false);
    scrolled
}

/// Build a scrollable text body for a message dialog.
pub fn scrollable_body(parent: Option<&gtk4::Window>, text: &str) -> gtk4::ScrolledWindow {
    let label = gtk4::Label::builder()
        .label(text)
        .wrap(true)
        .wrap_mode(gtk4::pango::WrapMode::WordChar)
        .xalign(0.0)
        .halign(gtk4::Align::Fill)
        .selectable(true)
        .build();
    label.add_css_class("dialog-body-text");

    let scrolled = scrollable_content(parent, &label);
    scrolled.set_margin_top(4);
    scrolled.set_margin_bottom(4);
    scrolled
}

/// Create a message dialog whose body always lives in a scrollable region.
///
/// Short bodies keep their natural height through
/// `propagate-natural-height`; long bodies stop at the window-relative cap and
/// show a vertical scrollbar instead of pushing the buttons off-screen.
pub fn message_dialog(
    parent: Option<&gtk4::Window>,
    heading: &str,
    body: &str,
) -> adw::MessageDialog {
    let dialog = adw::MessageDialog::new(parent, Some(heading), None);

    if !body.is_empty() {
        dialog.set_extra_child(Some(&scrollable_body(parent, body)));
    }

    prepare_window(&dialog);
    dialog
}
