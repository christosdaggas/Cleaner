use crate::storage::Storage;
use crate::theme;
use crate::ui::MainWindow;
use chrono::{Datelike, Local, Timelike};
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib, gdk};
use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DataCleanerApplication {
        pub storage: RefCell<Option<Arc<Storage>>>,
        pub window: RefCell<Option<MainWindow>>,
        pub tray_cmd: RefCell<Option<async_channel::Sender<crate::tray::TrayCmd>>>,
        pub tray_hint_shown: Cell<bool>,
        pub tray_available: Cell<bool>,
        pub start_hidden: Cell<bool>,
        pub last_scheduled_cleanup_slot: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DataCleanerApplication {
        const NAME: &'static str = "DataCleanerApplication";
        type Type = super::DataCleanerApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for DataCleanerApplication {
        fn constructed(&self) {
            self.parent_constructed();

            let app = self.obj();
            app.set_application_id(Some(crate::APP_ID));
            app.set_resource_base_path(Some("/com/chrisdaggas/datacleaner"));
            app.setup_actions();
        }
    }

    impl ApplicationImpl for DataCleanerApplication {
        fn activate(&self) {
            let app = self.obj();
            let start_hidden = self.start_hidden.replace(false);

            if self.storage.borrow().is_none() {
                let storage = Arc::new(Storage::new());
                crate::i18n::set_language(storage.get_settings().language);
                theme::apply_color_scheme(
                    &adw::StyleManager::default(),
                    storage.get_settings().color_scheme,
                );
                *self.storage.borrow_mut() = Some(storage);
            }

            if let Some(window) = self.window.borrow().as_ref() {
                if !start_hidden {
                    window.present();
                }
                return;
            }

            let storage = self.storage.borrow().as_ref().unwrap().clone();
            let window = MainWindow::new(&app, storage);
            window.set_hide_on_close(self.tray_available.get());

            let app_weak = app.downgrade();
            window.connect_close_request(move |window| {
                if window.operation_is_running() {
                    window.show_operation_running_dialog();
                    return glib::Propagation::Stop;
                }
                if let Some(app) = app_weak.upgrade() {
                    if window.property::<bool>("hide-on-close") {
                        app.notify_tray_hint_once();
                    }
                }
                glib::Propagation::Proceed
            });

            app.load_css();

            *self.window.borrow_mut() = Some(window.clone());
            if !start_hidden {
                window.present();
            }
        }

        fn startup(&self) {
            self.parent_startup();

            if self.storage.borrow().is_none() {
                let storage = Arc::new(Storage::new());
                crate::i18n::set_language(storage.get_settings().language);
                theme::apply_color_scheme(
                    &adw::StyleManager::default(),
                    storage.get_settings().color_scheme,
                );
                *self.storage.borrow_mut() = Some(storage);
            }

            if let Some(display) = gdk::Display::default() {
                let icon_theme = gtk4::IconTheme::for_display(&display);

                if let Ok(exe_path) = std::env::current_exe() {
                    if let Some(exe_dir) = exe_path.parent() {
                        let dev_icons = exe_dir.join("../../data/icons");
                        if dev_icons.exists() {
                            if let Some(path_str) = dev_icons.canonicalize().ok().and_then(|p| p.to_str().map(String::from)) {
                                icon_theme.add_search_path(&path_str);
                            }
                        }
                    }
                }

                icon_theme.add_search_path("data/icons");
            }

            gtk4::Window::set_default_icon_name(crate::APP_ID);
            self.obj().setup_tray();
            self.obj().setup_in_app_scheduler();
        }
    }

    impl GtkApplicationImpl for DataCleanerApplication {}
    impl AdwApplicationImpl for DataCleanerApplication {}
}

glib::wrapper! {
    pub struct DataCleanerApplication(ObjectSubclass<imp::DataCleanerApplication>)
        @extends gio::Application, gtk4::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl DataCleanerApplication {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", crate::APP_ID)
            .property("flags", gio::ApplicationFlags::FLAGS_NONE)
            .build()
    }

    pub fn set_start_hidden(&self, start_hidden: bool) {
        self.imp().start_hidden.set(start_hidden);
    }

    fn setup_actions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| {
                app.request_quit();
            })
            .build();

        let about_action = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| {
                app.show_about_dialog();
            })
            .build();

        let preferences_action = gio::ActionEntry::builder("preferences")
            .activate(|app: &Self, _, _| {
                app.show_preferences();
            })
            .build();

        let whats_new_action = gio::ActionEntry::builder("whats-new")
            .activate(|app: &Self, _, _| {
                app.show_whats_new_dialog();
            })
            .build();

        self.add_action_entries([
            quit_action,
            about_action,
            preferences_action,
            whats_new_action,
        ]);

        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
        self.set_accels_for_action("app.preferences", &["<Control>comma"]);
    }

    fn setup_tray(&self) {
        // Rasterised here, on the GTK main thread, because it goes through
        // gdk-pixbuf. The tray thread only ever receives finished pixels.
        let style_manager = adw::StyleManager::default();
        let (cmd_tx, action_rx, status_rx) =
            crate::tray::start_tray_service(crate::tray::render_tray_icons(style_manager.is_dark()));
        *self.imp().tray_cmd.borrow_mut() = Some(cmd_tx.clone());

        // The tray pixmap is pre-rendered, so it cannot follow the panel on its
        // own the way a host-resolved symbolic icon would. Re-render it on every
        // light/dark switch to keep the glyph legible against the panel.
        let theme_cmd_tx = cmd_tx.clone();
        style_manager.connect_dark_notify(move |style_manager| {
            let icons = crate::tray::render_tray_icons(style_manager.is_dark());
            let _ = theme_cmd_tx.try_send(crate::tray::TrayCmd::SetIcons(icons));
        });

        let app_weak = self.downgrade();
        glib::spawn_future_local(async move {
            while let Ok(action) = action_rx.recv().await {
                let Some(app) = app_weak.upgrade() else {
                    break;
                };
                app.on_tray_action(action);
            }
        });

        let app_weak = self.downgrade();
        glib::spawn_future_local(async move {
            while let Ok(status) = status_rx.recv().await {
                let Some(app) = app_weak.upgrade() else {
                    break;
                };
                let available = matches!(status, crate::tray::TrayStatus::Available);
                app.imp().tray_available.set(available);
                let window = app.imp().window.borrow().as_ref().cloned();
                if let Some(window) = window {
                    window.set_hide_on_close(available);
                }
            }
        });

        let _ = cmd_tx.try_send(crate::tray::TrayCmd::Enable);
    }

    fn setup_in_app_scheduler(&self) {
        let app_weak = self.downgrade();
        glib::timeout_add_seconds_local(15, move || {
            let Some(app) = app_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };

            app.run_scheduled_cleanup_if_due();
            glib::ControlFlow::Continue
        });
    }

    fn run_scheduled_cleanup_if_due(&self) {
        let Some(storage) = self.storage() else {
            return;
        };
        let settings = storage.get_settings();
        if !settings.scheduled_cleanup_enabled {
            return;
        }

        let now = Local::now();
        if !settings.scheduled_cleanup_matches(now.weekday())
            || settings.scheduled_cleanup_hour != now.hour() as u8
            || settings.scheduled_cleanup_minute != now.minute() as u8
        {
            return;
        }

        let slot = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute()
        );
        if self
            .imp()
            .last_scheduled_cleanup_slot
            .borrow()
            .as_deref()
            == Some(slot.as_str())
        {
            return;
        }

        let started = self
            .imp()
            .window
            .borrow()
            .as_ref()
            .map(MainWindow::request_scheduled_clean)
            .unwrap_or(false);
        if started {
            tracing::info!(schedule_slot = %slot, "Starting scheduled cleanup");
            self.imp()
                .last_scheduled_cleanup_slot
                .replace(Some(slot));
        }
    }

    fn on_tray_action(&self, action: crate::tray::TrayAction) {
        match action {
            crate::tray::TrayAction::Open => self.present_main_window(),
            crate::tray::TrayAction::Clean => {
                self.present_main_window();
                if let Some(window) = self.imp().window.borrow().as_ref() {
                    window.request_clean();
                }
            }
            crate::tray::TrayAction::Close => self.request_quit(),
        }
    }

    fn present_main_window(&self) {
        if let Some(window) = self.imp().window.borrow().as_ref() {
            window.present();
        } else {
            self.activate();
        }
    }

    fn notify_tray_hint_once(&self) {
        if self.imp().tray_hint_shown.replace(true) {
            return;
        }

        let notification = gio::Notification::new(crate::APP_NAME);
        notification.set_body(Some(
            "Data Cleaner is still running in the system tray. Use the tray menu's Close action to exit.",
        ));
        notification.set_icon(&gio::ThemedIcon::new(crate::APP_ID));
        self.send_notification(Some("tray-hint"), &notification);
    }

    fn request_quit(&self) {
        if let Some(window) = self.imp().window.borrow().as_ref() {
            if window.operation_is_running() {
                window.present();
                window.show_operation_running_dialog();
                return;
            }
        }

        if let Some(sender) = self.imp().tray_cmd.borrow().as_ref() {
            let _ = sender.try_send(crate::tray::TrayCmd::Disable);
        }
        self.quit();
    }

    fn load_css(&self) {
        let provider = gtk4::CssProvider::new();
        Self::load_css_provider(&provider);

        let Some(display) = gtk4::gdk::Display::default() else {
            tracing::warn!("Could not get default display; skipping CSS provider registration");
            return;
        };
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        self.refresh_theme_state();

        let style_manager = adw::StyleManager::default();
        let app_weak = self.downgrade();
        style_manager.connect_color_scheme_notify(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.refresh_theme_state();
                app.queue_redraw_all_windows();
            }
        });

        if style_manager.find_property("accent-color").is_some() {
            let app_weak = self.downgrade();
            style_manager.connect_notify_local(Some("accent-color"), move |_, _| {
                if let Some(app) = app_weak.upgrade() {
                    app.refresh_theme_state();
                    app.queue_redraw_all_windows();
                }
            });
        }

        let app_weak = self.downgrade();
        style_manager.connect_dark_notify(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.refresh_theme_state();
                app.queue_redraw_all_windows();
            }
        });

        let app_weak = self.downgrade();
        style_manager.connect_high_contrast_notify(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.refresh_theme_state();
                app.queue_redraw_all_windows();
            }
        });

        let Some(gtk_settings) = gtk4::Settings::default() else {
            tracing::warn!("Could not get GTK settings; skipping settings notifications");
            return;
        };

        let app_weak = self.downgrade();
        gtk_settings.connect_gtk_theme_name_notify(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.refresh_theme_state();
                app.queue_redraw_all_windows();
            }
        });

        let app_weak = self.downgrade();
        gtk_settings.connect_gtk_icon_theme_name_notify(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.refresh_theme_state();
                app.queue_redraw_all_windows();
            }
        });

        let app_weak = self.downgrade();
        gtk_settings.connect_gtk_application_prefer_dark_theme_notify(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.refresh_theme_state();
                app.queue_redraw_all_windows();
            }
        });

        let app_weak = self.downgrade();
        gtk_settings.connect_gtk_enable_animations_notify(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.refresh_theme_state();
                app.queue_redraw_all_windows();
            }
        });
    }

    fn queue_redraw_all_windows(&self) {
        for window in self.windows() {
            window.queue_draw();
        }
    }

    fn refresh_theme_state(&self) {
        for window in self.windows() {
            if let Ok(main_window) = window.clone().downcast::<MainWindow>() {
                main_window.sync_theme_preferences();
            } else {
                theme::sync_runtime_classes(&window);
            }
        }
    }

    fn load_css_provider(provider: &gtk4::CssProvider) {
        if gio::resources_lookup_data(
            "/com/chrisdaggas/datacleaner/style.css",
            gio::ResourceLookupFlags::NONE,
        ).is_ok() {
            provider.load_from_resource("/com/chrisdaggas/datacleaner/style.css");
        } else {
            provider.load_from_data(include_str!("../data/style.css"));
        }
    }

    fn show_about_dialog(&self) {
        let window = self.active_window();

        let dialog = gtk4::AboutDialog::builder()
            .program_name(crate::APP_NAME)
            .logo_icon_name(crate::APP_ID)
            .version(crate::DISPLAY_VERSION)
            .website("https://chrisdaggas.com")
            .license_type(gtk4::License::Gpl30)
            .copyright("© 2024-2026 Christos A. Daggas")
            .comments(crate::i18n::tr(
                "A safe and transparent Linux system cleaner",
            ))
            .authors(vec!["Christos A. Daggas"])
            .website_label(crate::i18n::tr("Project Website"))
            .modal(true)
            .build();

        if let Some(window) = window.as_ref() {
            dialog.set_transient_for(Some(window));
        }

        crate::i18n::translate_widget_tree(&dialog);
        dialog.present();
    }

    fn show_whats_new_dialog(&self) {
        let content = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(18)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();

        for (index, release) in crate::release_notes::RELEASES.iter().enumerate() {
            if index > 0 {
                content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
            }

            let heading = gtk4::Label::new(Some(&format!(
                "{} {}",
                crate::i18n::tr("Version"),
                release.version
            )));
            heading.set_halign(gtk4::Align::Start);
            heading.add_css_class("title-2");
            content.append(&heading);

            let metadata = gtk4::Label::new(Some(&format!(
                "{} · {}",
                crate::i18n::tr(release.title),
                release.date
            )));
            metadata.set_halign(gtk4::Align::Start);
            metadata.add_css_class("dim-label");
            content.append(&metadata);

            for change in release.changes {
                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
                row.set_valign(gtk4::Align::Start);

                let bullet = gtk4::Label::new(Some("•"));
                bullet.add_css_class("accent");
                row.append(&bullet);

                let description = gtk4::Label::new(Some(&crate::i18n::tr(change)));
                description.set_halign(gtk4::Align::Start);
                description.set_xalign(0.0);
                description.set_wrap(true);
                description.set_hexpand(true);
                row.append(&description);
                content.append(&row);
            }
        }

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .min_content_height(340)
            .max_content_height(460)
            .min_content_width(520)
            .propagate_natural_height(true)
            .child(&content)
            .build();

        let dialog = adw::MessageDialog::new(
            self.active_window().as_ref(),
            Some(&crate::i18n::tr("What's New")),
            Some(&crate::i18n::tr("Release history and highlights")),
        );
        dialog.set_extra_child(Some(&scrolled));
        dialog.add_response("close", &crate::i18n::tr("Close"));
        dialog.set_default_response(Some("close"));
        dialog.set_close_response("close");
        dialog.present();
    }

    fn show_preferences(&self) {
        if let Some(window) = self.imp().window.borrow().as_ref() {
            window.navigate_to_settings();
        }
    }

    pub fn storage(&self) -> Option<Arc<Storage>> {
        self.imp().storage.borrow().as_ref().cloned()
    }
}

impl Default for DataCleanerApplication {
    fn default() -> Self {
        Self::new()
    }
}
