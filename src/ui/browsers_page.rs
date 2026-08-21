use crate::models::{BrowserDataType, BrowserRule, BrowserType};
use crate::storage::Storage;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::sync::Arc;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct BrowsersPage {
        pub storage: RefCell<Option<Arc<Storage>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BrowsersPage {
        const NAME: &'static str = "DataCleanerBrowsersPage";
        type Type = super::BrowsersPage;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for BrowsersPage {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for BrowsersPage {}
    impl BoxImpl for BrowsersPage {}
}

glib::wrapper! {
    pub struct BrowsersPage(ObjectSubclass<imp::BrowsersPage>)
        @extends gtk4::Widget, gtk4::Box;
}

impl BrowsersPage {
    pub fn new(storage: Arc<Storage>) -> Self {
        let page: Self = glib::Object::builder()
            .property("orientation", gtk4::Orientation::Vertical)
            .property("spacing", 0)
            .build();

        page.imp().storage.replace(Some(storage));
        page.setup_ui();
        page
    }

    fn setup_ui(&self) {
        // Scrolled container - matches Network Manager layout
        let scrolled = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .build();
        self.append(&scrolled);

        // Content box with margins - matching Network Manager settings page
        let content = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .hexpand(true)
            .build();
        scrolled.set_child(Some(&content));

        // Header
        let header = self.create_header();
        content.append(&header);

        // Keep the complete supported-browser catalog visible. Unavailable
        // browsers are rendered insensitive inside their individual section.
        for browser in BrowserType::all() {
            let section = self.create_browser_section(*browser);
            content.append(&section);
        }
    }

    fn create_header(&self) -> gtk4::Widget {
        let header_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);

        let title = gtk4::Label::new(Some("Browser Cleanup"));
        title.add_css_class("title-2");
        title.set_halign(gtk4::Align::Start);

        let description = gtk4::Label::new(Some(
            "Installed browsers can be configured below. Browsers that are not installed remain visible but unavailable.",
        ));
        description.add_css_class("dim-label");
        description.set_halign(gtk4::Align::Start);
        description.set_wrap(true);
        description.set_xalign(0.0);

        header_box.append(&title);
        header_box.append(&description);

        header_box.upcast()
    }

    fn create_browser_section(&self, browser: BrowserType) -> gtk4::Widget {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();
        let rules = storage.get_browser_rules();
        let installed = browser.is_installed();

        let expander = adw::ExpanderRow::new();
        expander.set_title(browser.display_name());
        expander.set_subtitle(if installed {
            "Installed"
        } else {
            "Not installed"
        });
        expander.set_sensitive(installed);

        // Load browser icon from resources
        let icon = gtk4::Image::new();
        icon.set_pixel_size(32);

        // Try to load from embedded resources first
        let resource_path = browser.icon_resource();
        if gio::resources_get_info(resource_path, gio::ResourceLookupFlags::NONE).is_ok() {
            icon.set_resource(Some(resource_path));
        } else {
            // Fallback to system icon
            icon.set_icon_name(Some(browser.icon_name()));
        }

        expander.add_prefix(&icon);
        expander.set_enable_expansion(installed);

        // Find rules for this browser
        for data_type in BrowserDataType::all() {
            let rule = rules.iter().find(|r| r.browser == browser && r.data_type == *data_type);
            let is_enabled = rule.map(|r| r.enabled).unwrap_or(false);

            let row = adw::ActionRow::new();
            row.set_title(data_type.display_name());
            row.set_subtitle(data_type.description());

            if data_type.is_sensitive() {
                row.add_css_class("warning");
            }

            let switch = gtk4::Switch::new();
            switch.set_active(is_enabled);
            switch.set_valign(gtk4::Align::Center);

            // Handle switch toggle
            let storage_clone = storage.clone();
            let browser_type = browser;
            let dt = *data_type;
            switch.connect_active_notify(move |sw| {
                let enabled = sw.is_active();
                if let Err(e) = storage_clone.update_browser_rules(|rules| {
                    if let Some(rule) = rules.iter_mut().find(|r| r.browser == browser_type && r.data_type == dt) {
                        rule.enabled = enabled;
                    } else {
                        let mut rule = BrowserRule::new(browser_type, dt);
                        rule.enabled = enabled;
                        rules.push(rule);
                    }
                }) {
                    tracing::warn!("Failed to update browser rules: {}", e);
                }
            });

            row.add_suffix(&switch);
            row.set_activatable_widget(Some(&switch));

            expander.add_row(&row);
        }

        // Wrap in a preferences group for styling
        let group = adw::PreferencesGroup::new();
        group.add(&expander);

        group.upcast()
    }
}
