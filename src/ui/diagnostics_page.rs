// Cleaner - Diagnostics Page
// Copyright (C) 2026 Christos A. Daggas
// SPDX-License-Identifier: GPL-3.0-or-later

//! Diagnostics Page - System compatibility and environment information.
//!
//! This page reports:
//! - Detected desktop environment
//! - Wayland/X11 display server
//! - Theme mode (dark/light)
//! - Icon resolution status
//! - XDG portal availability
//! - Protected paths summary

use crate::services::{DesktopEnvironment, DisplayServer, IconDiagnostics, IconResolver};
use crate::i18n::{tr, tr_args};
use crate::theme::ThemeSnapshot;
use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DiagnosticsPage {
        pub icon_diagnostics: RefCell<Option<IconDiagnostics>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DiagnosticsPage {
        const NAME: &'static str = "DiagnosticsPage";
        type Type = super::DiagnosticsPage;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for DiagnosticsPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_ui();
        }
    }

    impl WidgetImpl for DiagnosticsPage {}
    impl BoxImpl for DiagnosticsPage {}
}

glib::wrapper! {
    pub struct DiagnosticsPage(ObjectSubclass<imp::DiagnosticsPage>)
        @extends gtk::Widget, gtk::Box;
}

impl DiagnosticsPage {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("orientation", gtk::Orientation::Vertical)
            .property("spacing", 0)
            .build()
    }

    fn setup_ui(&self) {
        // Page header
        let header_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        header_box.set_margin_start(24);
        header_box.set_margin_end(24);
        header_box.set_margin_top(24);
        header_box.set_margin_bottom(12);

        let title = gtk::Label::new(Some(&tr("Diagnostics")));
        title.add_css_class("title-1");
        title.set_halign(gtk::Align::Start);
        header_box.append(&title);

        let subtitle = gtk::Label::new(Some(&tr(
            "System compatibility and environment information",
        )));
        subtitle.add_css_class("dim-label");
        subtitle.set_halign(gtk::Align::Start);
        header_box.append(&subtitle);

        self.append(&header_box);

        // Scrollable content
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_hexpand(true);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 18);
        content_box.set_margin_start(24);
        content_box.set_margin_end(24);
        content_box.set_margin_top(12);
        content_box.set_margin_bottom(24);

        // Environment section
        content_box.append(&self.create_environment_section());

        // Display section
        content_box.append(&self.create_display_section());

        // Theme section
        content_box.append(&self.create_theme_section());

        // Icon section
        content_box.append(&self.create_icon_section());

        // Portals section
        content_box.append(&self.create_portals_section());

        // Paths section
        content_box.append(&self.create_paths_section());

        scroll.set_child(Some(&content_box));
        self.append(&scroll);
    }

    fn create_environment_section(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title(&tr("Desktop Environment"));
        group.set_description(Some(&tr("Information about your desktop environment")));

        let de = DesktopEnvironment::detect();
        let de_row = adw::ActionRow::builder()
            .title(tr("Desktop Environment"))
            .subtitle(de.display_name())
            .build();
        de_row.add_prefix(&self.create_status_icon(true));
        group.add(&de_row);

        // XDG_CURRENT_DESKTOP value
        let xdg_desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| tr("Not set"));
        let xdg_row = adw::ActionRow::builder()
            .title("XDG_CURRENT_DESKTOP")
            .subtitle(&xdg_desktop)
            .build();
        group.add(&xdg_row);

        // Session type
        let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| tr("Unknown"));
        let session_row = adw::ActionRow::builder()
            .title("XDG_SESSION_TYPE")
            .subtitle(&session_type)
            .build();
        group.add(&session_row);

        group
    }

    fn create_display_section(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title(&tr("Display Server"));
        group.set_description(Some(&tr("Information about the display server")));

        let display_server = DesktopEnvironment::display_server();
        let is_wayland = display_server == DisplayServer::Wayland;

        let server_row = adw::ActionRow::builder()
            .title(tr("Display Server"))
            .subtitle(display_server.display_name())
            .build();
        server_row.add_prefix(&self.create_status_icon(true));
        group.add(&server_row);

        // Wayland display
        if is_wayland {
            let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| tr("Not set"));
            let wayland_row = adw::ActionRow::builder()
                .title("WAYLAND_DISPLAY")
                .subtitle(&wayland_display)
                .build();
            group.add(&wayland_row);
        } else {
            // X11 display
            let x_display = std::env::var("DISPLAY").unwrap_or_else(|_| tr("Not set"));
            let x_row = adw::ActionRow::builder()
                .title("DISPLAY")
                .subtitle(&x_display)
                .build();
            group.add(&x_row);
        }

        // Compatibility note
        let compat_row = adw::ActionRow::builder()
            .title(tr("Compatibility"))
            .subtitle(if is_wayland {
                tr("Full Wayland support via GTK4")
            } else {
                tr("X11 mode - all features available")
            })
            .build();
        compat_row.add_prefix(&self.create_status_icon(true));
        group.add(&compat_row);

        group
    }

    fn create_theme_section(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title(&tr("Theme"));
        group.set_description(Some(&tr("Current theme and color scheme settings")));
        let theme = ThemeSnapshot::from_widget(self);

        // Get current icon theme
        let icon_theme = IconResolver::get_current_theme_name()
            .unwrap_or_else(|| tr("Unknown"));
        let icon_theme_row = adw::ActionRow::builder()
            .title(tr("Icon Theme"))
            .subtitle(&icon_theme)
            .build();
        group.add(&icon_theme_row);

        // System color scheme
        let style_manager = adw::StyleManager::default();
        let color_scheme = if style_manager.is_dark() {
            tr("Dark")
        } else {
            tr("Light")
        };
        let scheme_row = adw::ActionRow::builder()
            .title(tr("Color Scheme"))
            .subtitle(color_scheme)
            .build();
        group.add(&scheme_row);

        // System preference
        let system_pref = match style_manager.color_scheme() {
            adw::ColorScheme::Default => tr("Follow System"),
            adw::ColorScheme::ForceLight => tr("Force Light"),
            adw::ColorScheme::ForceDark => tr("Force Dark"),
            adw::ColorScheme::PreferLight => tr("Prefer Light"),
            adw::ColorScheme::PreferDark => tr("Prefer Dark"),
            _ => tr("Unknown"),
        };
        let pref_row = adw::ActionRow::builder()
            .title(tr("Theme Mode"))
            .subtitle(system_pref)
            .build();
        group.add(&pref_row);

        // High contrast
        let hc_row = adw::ActionRow::builder()
            .title(tr("High Contrast"))
            .subtitle(if theme.is_high_contrast { tr("Enabled") } else { tr("Disabled") })
            .build();
        hc_row.add_prefix(&self.create_status_icon(true));
        group.add(&hc_row);

        let motion_row = adw::ActionRow::builder()
            .title(tr("Reduced Motion"))
            .subtitle(if theme.reduced_motion { tr("Enabled") } else { tr("Disabled") })
            .build();
        motion_row.add_prefix(&self.create_status_icon(true));
        group.add(&motion_row);

        let accent_row = adw::ActionRow::builder()
            .title(tr("Accent Color"))
            .subtitle(theme.accent_bg.to_str().as_str())
            .build();
        accent_row.add_prefix(&self.create_status_icon(true));
        group.add(&accent_row);

        group
    }

    fn create_icon_section(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title(&tr("Icon Resolution"));
        group.set_description(Some(&tr("Status of icon loading and fallbacks")));

        // Test a few common icons
        let mut resolver = IconResolver::new();
        let test_icons = [
            "web-browser-symbolic",
            "folder-symbolic",
            "user-trash-symbolic",
            "computer-symbolic",
            "preferences-system-symbolic",
        ];

        for icon_name in test_icons {
            let resolution = resolver.resolve(icon_name);
            let row = adw::ActionRow::builder()
                .title(icon_name)
                .subtitle(resolution.resolution_type.display_name())
                .build();
            row.add_prefix(&self.create_status_icon(!resolution.resolution_type.is_fallback()));
            group.add(&row);
        }

        // Summary
        let diagnostics = resolver.get_diagnostics_summary();
        *self.imp().icon_diagnostics.borrow_mut() = Some(diagnostics.clone());

        let summary_row = adw::ActionRow::builder()
            .title(tr("Resolution Rate"))
            .subtitle(tr_args(
                "{percent}% icons resolved from theme",
                &[("{percent}", &format!("{:.0}", diagnostics.success_rate()))],
            ))
            .build();
        summary_row.add_prefix(&self.create_status_icon(diagnostics.all_resolved()));
        group.add(&summary_row);

        group
    }

    fn create_portals_section(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title(&tr("XDG Portals"));
        group.set_description(Some(&tr("Desktop portal availability for sandboxed access")));

        // Check if running in Flatpak
        let in_flatpak = std::path::Path::new("/.flatpak-info").exists();
        let flatpak_row = adw::ActionRow::builder()
            .title(tr("Flatpak Environment"))
            .subtitle(if in_flatpak {
                tr("Running in Flatpak sandbox")
            } else {
                tr("Native installation")
            })
            .build();
        flatpak_row.add_prefix(&self.create_status_icon(true));
        group.add(&flatpak_row);

        // Portal backend (from env)
        let portal_backend = std::env::var("GTK_USE_PORTAL")
            .map(|v| if v == "1" { tr("Enabled") } else { tr("Disabled") })
            .unwrap_or_else(|_| tr("Default"));
        let portal_row = adw::ActionRow::builder()
            .title(tr("GTK Portal Usage"))
            .subtitle(portal_backend)
            .build();
        group.add(&portal_row);

        // XDG_DATA_DIRS for portal lookup
        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/share:/usr/local/share".to_string());
        let has_portal_data = data_dirs.contains("flatpak") ||
            std::path::Path::new("/usr/share/xdg-desktop-portal").exists();

        let portal_installed_row = adw::ActionRow::builder()
            .title(tr("Portal Service"))
            .subtitle(if has_portal_data {
                tr("Available")
            } else {
                tr("Not detected")
            })
            .build();
        portal_installed_row.add_prefix(&self.create_status_icon(has_portal_data));
        group.add(&portal_installed_row);

        group
    }

    fn create_paths_section(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title(&tr("XDG Paths"));
        group.set_description(Some(&tr("Standard directory locations being used")));

        let paths = [
            (tr("Config"), dirs::config_dir()),
            (tr("Cache"), dirs::cache_dir()),
            (tr("Data"), dirs::data_dir()),
            (tr("Home"), dirs::home_dir()),
        ];

        for (name, path) in paths {
            let path_str = path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| tr("Not available"));

            let exists = path.as_ref().map(|p| p.exists()).unwrap_or(false);

            let row = adw::ActionRow::builder()
                .title(&name)
                .subtitle(&path_str)
                .build();
            row.add_prefix(&self.create_status_icon(exists));
            group.add(&row);
        }

        // App config directory
        let app_config = dirs::config_dir()
            .map(|p| p.join("data-cleaner"))
            .unwrap_or_default();
        let app_config_str = app_config.display().to_string();
        let app_row = adw::ActionRow::builder()
            .title(tr("App Config"))
            .subtitle(&app_config_str)
            .build();
        app_row.add_prefix(&self.create_status_icon(app_config.exists()));
        group.add(&app_row);

        group
    }

    fn create_status_icon(&self, success: bool) -> gtk::Image {
        let icon = gtk::Image::from_icon_name(if success {
            "object-select-symbolic"
        } else {
            "dialog-warning-symbolic"
        });
        icon.add_css_class(if success { "success" } else { "warning" });
        icon
    }

    /// Update icon diagnostics from an external resolver
    pub fn update_icon_diagnostics(&self, diagnostics: IconDiagnostics) {
        *self.imp().icon_diagnostics.borrow_mut() = Some(diagnostics);
    }
}

impl Default for DiagnosticsPage {
    fn default() -> Self {
        Self::new()
    }
}
