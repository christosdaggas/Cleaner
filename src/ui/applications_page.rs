use crate::models::AppRule;
use crate::storage::Storage;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::sync::Arc;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ApplicationsPage {
        pub storage: RefCell<Option<Arc<Storage>>>,
        pub list_box: RefCell<Option<gtk4::ListBox>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ApplicationsPage {
        const NAME: &'static str = "DataCleanerApplicationsPage";
        type Type = super::ApplicationsPage;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for ApplicationsPage {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for ApplicationsPage {}
    impl BoxImpl for ApplicationsPage {}
}

glib::wrapper! {
    pub struct ApplicationsPage(ObjectSubclass<imp::ApplicationsPage>)
        @extends gtk4::Widget, gtk4::Box;
}

impl ApplicationsPage {
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

        // Application rules list
        let rules_group = self.create_rules_list();
        content.append(&rules_group);
    }

    fn create_header(&self) -> gtk4::Widget {
        let header_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);

        let title = gtk4::Label::new(Some("Application Caches"));
        title.add_css_class("title-2");
        title.set_halign(gtk4::Align::Start);

        let description = gtk4::Label::new(Some(
            "Clean caches from package managers and applications. These are safe to delete but may slow down first runs.",
        ));
        description.add_css_class("dim-label");
        description.set_halign(gtk4::Align::Start);
        description.set_wrap(true);
        description.set_xalign(0.0);

        header_box.append(&title);
        header_box.append(&description);

        header_box.upcast()
    }

    fn create_rules_list(&self) -> gtk4::Widget {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();
        let rules = storage.get_app_rules();

        let group = adw::PreferencesGroup::new();
        group.set_title("Available Caches");
        group.set_description(Some("Toggle caches you want to include in cleanup"));

        for rule in &rules {
            let row = self.create_rule_row(rule);
            group.add(&row);
        }

        group.upcast()
    }

    fn create_rule_row(&self, rule: &AppRule) -> adw::ActionRow {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();

        // Check if path exists
        let exists = rule.expanded_path().map(|p| p.exists()).unwrap_or(false);

        let row = adw::ActionRow::new();
        row.set_title(&rule.name);
        row.set_subtitle(&format!("{}\n{}", rule.description, rule.path.display()));

        if !exists {
            row.add_css_class("dim-label");
        }

        // Enable/disable switch
        let switch = gtk4::Switch::new();
        switch.set_active(rule.enabled);
        switch.set_valign(gtk4::Align::Center);
        switch.set_sensitive(exists);

        let rule_id = rule.id;
        switch.connect_active_notify(move |sw| {
            let enabled = sw.is_active();
            if let Err(e) = storage.update_app_rules(|rules| {
                if let Some(r) = rules.iter_mut().find(|r| r.id == rule_id) {
                    r.enabled = enabled;
                }
            }) {
                tracing::warn!("Failed to update app rules: {}", e);
            }
        });

        row.add_suffix(&switch);
        row.set_activatable_widget(Some(&switch));

        row
    }
}
