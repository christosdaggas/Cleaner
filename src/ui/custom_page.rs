use crate::models::{CustomRule, DeletionMode};
use crate::services::SecurityAuditor;
use crate::storage::Storage;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct CustomPage {
        pub storage: RefCell<Option<Arc<Storage>>>,
        pub list_box: RefCell<Option<adw::PreferencesGroup>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CustomPage {
        const NAME: &'static str = "DataCleanerCustomPage";
        type Type = super::CustomPage;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for CustomPage {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for CustomPage {}
    impl BoxImpl for CustomPage {}
}

glib::wrapper! {
    pub struct CustomPage(ObjectSubclass<imp::CustomPage>)
        @extends gtk4::Widget, gtk4::Box;
}

impl CustomPage {
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

        // Add button
        let add_button = self.create_add_button();
        content.append(&add_button);

        // Rules list
        let rules_group = self.create_rules_list();
        content.append(&rules_group);
    }

    fn create_header(&self) -> gtk4::Widget {
        let header_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);

        let title = gtk4::Label::new(Some("Custom Directories"));
        title.add_css_class("title-2");
        title.set_halign(gtk4::Align::Start);

        let description = gtk4::Label::new(Some(
            "Add your own directories to clean. Use with caution - these rules can delete important files if misconfigured.",
        ));
        description.add_css_class("dim-label");
        description.set_halign(gtk4::Align::Start);
        description.set_wrap(true);
        description.set_xalign(0.0);

        let warning = gtk4::Label::new(Some(
            "Advanced setting: verify every custom path before enabling cleanup.",
        ));
        warning.add_css_class("warning");
        warning.add_css_class("caption");
        warning.set_halign(gtk4::Align::Start);
        warning.set_wrap(true);
        warning.set_xalign(0.0);

        header_box.append(&title);
        header_box.append(&description);
        header_box.append(&warning);

        header_box.upcast()
    }

    fn create_add_button(&self) -> gtk4::Widget {
        let button_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        button_box.set_halign(gtk4::Align::Start);

        let add_button = gtk4::Button::new();
        let add_button_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        add_button_content.append(&gtk4::Image::from_icon_name("list-add-symbolic"));
        add_button_content.append(&gtk4::Label::new(Some("Add Rule")));
        add_button.set_child(Some(&add_button_content));
        add_button.add_css_class("suggested-action");

        let page = self.downgrade();
        add_button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                page.show_add_dialog();
            }
        });

        button_box.append(&add_button);
        button_box.upcast()
    }

    fn create_rules_list(&self) -> gtk4::Widget {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();
        let rules = storage.get_custom_rules();

        let group = adw::PreferencesGroup::new();
        group.set_title("Custom Rules");
        group.set_description(Some("Your custom cleanup directories"));

        if rules.is_empty() {
            let row = adw::ActionRow::new();
            row.set_title("No custom rules");
            row.set_subtitle("Click 'Add Rule' to create one");
            row.add_css_class("dim-label");
            group.add(&row);
        } else {
            for rule in &rules {
                let row = self.create_rule_row(rule);
                group.add(&row);
            }
        }

        self.imp().list_box.replace(Some(group.clone()));
        group.upcast()
    }

    fn create_rule_row(&self, rule: &CustomRule) -> adw::ExpanderRow {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();

        let expander = adw::ExpanderRow::new();
        expander.set_title(&rule.name);

        let search_name = rule
            .subfolder_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        if let Some(search_name) = &search_name {
            expander.set_subtitle(&format!(
                "{} \u{2022} searching subfolders matching \"{}\"",
                rule.path.display(),
                search_name
            ));
        } else {
            expander.set_subtitle(&rule.path.display().to_string());
        }

        let enabled_row = adw::ActionRow::new();
        enabled_row.set_title("Enabled");

        let switch = gtk4::Switch::new();
        switch.set_active(rule.enabled);
        switch.set_valign(gtk4::Align::Center);

        let rule_id = rule.id;
        let storage_clone = storage.clone();
        switch.connect_active_notify(move |sw| {
            let enabled = sw.is_active();
            if let Err(e) = storage_clone.update_custom_rules(|rules| {
                if let Some(r) = rules.iter_mut().find(|r| r.id == rule_id) {
                    r.enabled = enabled;
                }
            }) {
                tracing::warn!("Failed to update custom rules: {}", e);
            }
        });

        enabled_row.add_suffix(&switch);
        enabled_row.set_activatable_widget(Some(&switch));
        expander.add_row(&enabled_row);

        let delete_row = adw::ActionRow::new();
        delete_row.set_title("Delete Rule");
        delete_row.set_subtitle("Remove this custom cleanup rule");

        let delete_button = gtk4::Button::from_icon_name("user-trash-symbolic");
        delete_button.add_css_class("flat");
        delete_button.add_css_class("destructive-action");
        delete_button.set_valign(gtk4::Align::Center);
        delete_button.set_tooltip_text(Some("Delete this rule"));

        let page = self.downgrade();
        let rule_id_delete = rule.id;
        delete_button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                page.delete_rule(rule_id_delete);
            }
        });

        delete_row.add_suffix(&delete_button);
        expander.add_row(&delete_row);

        // Details rows
        let mode_row = adw::ActionRow::new();
        mode_row.set_title("Deletion Mode");
        mode_row.set_subtitle(rule.deletion_mode.description());
        expander.add_row(&mode_row);

        if let Some(search_name) = &search_name {
            let search_row = adw::ActionRow::new();
            search_row.set_title("Subfolder Search");
            search_row.set_subtitle(&format!(
                "Rescans \"{}\" for folders matching \"{}\" on every scan",
                rule.path.display(),
                search_name
            ));
            expander.add_row(&search_row);
        }

        if let Some(pattern) = &rule.file_pattern {
            let pattern_row = adw::ActionRow::new();
            pattern_row.set_title("File Pattern");
            pattern_row.set_subtitle(pattern);
            expander.add_row(&pattern_row);
        }

        if rule.min_age_days > 0 {
            let age_row = adw::ActionRow::new();
            age_row.set_title("Minimum Age");
            age_row.set_subtitle(&format!("{} days", rule.min_age_days));
            expander.add_row(&age_row);
        }

        expander
    }

    fn show_add_dialog(&self) {
        let parent_window = self.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
        let form_window = gtk4::Window::builder()
            .title("Add Custom Rule")
            .default_width(580)
            .default_height(620)
            .resizable(true)
            .modal(false)
            .build();
        if let Some(application) = parent_window
            .as_ref()
            .and_then(|window| window.application())
        {
            form_window.set_application(Some(&application));
        }

        let header = adw::HeaderBar::new();
        header.add_css_class("custom-rule-header");
        let title = adw::WindowTitle::new("Add Custom Rule", "");
        header.set_title_widget(Some(&title));

        let cancel_button = gtk4::Button::with_label("Cancel");
        let form_window_weak = form_window.downgrade();
        cancel_button.connect_clicked(move |_| {
            if let Some(form_window) = form_window_weak.upgrade() {
                form_window.close();
            }
        });

        let add_button = gtk4::Button::with_label("Add Rule");
        add_button.add_css_class("suggested-action");
        form_window.set_titlebar(Some(&header));

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);

        // Name entry
        let name_group = adw::PreferencesGroup::new();
        let name_entry = adw::EntryRow::new();
        name_entry.set_title("Rule Name");
        name_group.add(&name_entry);
        content.append(&name_group);

        // Path entry
        let path_group = adw::PreferencesGroup::new();
        let path_entry = adw::EntryRow::new();
        path_entry.set_title("Directory Path");
        path_entry.set_text("~/");

        let choose_path_button = gtk4::Button::from_icon_name("folder-open-symbolic");
        choose_path_button.add_css_class("flat");
        choose_path_button.set_valign(gtk4::Align::Center);
        choose_path_button.set_tooltip_text(Some("Choose a directory"));
        path_entry.add_suffix(&choose_path_button);

        let form_window_weak = form_window.downgrade();
        let path_entry_for_picker = path_entry.clone();
        choose_path_button.connect_clicked(move |_| {
            let Some(form_window) = form_window_weak.upgrade() else {
                return;
            };
            let chooser = gtk4::FileChooserNative::new(
                Some("Choose a Directory"),
                Some(&form_window),
                gtk4::FileChooserAction::SelectFolder,
                Some("Select"),
                Some("Cancel"),
            );

            if let Some(current_path) =
                Self::expand_custom_path(path_entry_for_picker.text().as_str())
            {
                let _ = chooser.set_current_folder(Some(&gtk4::gio::File::for_path(current_path)));
            }

            let path_entry = path_entry_for_picker.clone();
            chooser.run_async(move |chooser, response| {
                if response == gtk4::ResponseType::Accept {
                    if let Some(path) = chooser.file().and_then(|file| file.path()) {
                        path_entry.set_text(&Self::display_custom_path(&path));
                    }
                }
                chooser.destroy();
            });
        });

        path_group.add(&path_entry);
        content.append(&path_group);

        // Optional subfolder search (e.g., "target" or "target/debug")
        let search_group = adw::PreferencesGroup::new();
        search_group.set_description(Some(
            "Leave empty to clean this exact path. When set, every matching subfolder is cleaned instead \u{2014} use a name like \"target\" or a relative path like \"target/debug\".",
        ));
        let search_entry = adw::EntryRow::new();
        search_entry.set_title("Search Subfolders Matching");
        search_group.add(&search_entry);
        content.append(&search_group);

        // Deletion mode
        let mode_group = adw::PreferencesGroup::new();
        mode_group.set_title("Deletion Mode");

        let files_only = gtk4::CheckButton::with_label("Files only (keep directory structure)");
        files_only.set_active(true);
        mode_group.add(&files_only);

        let files_and_dirs = gtk4::CheckButton::with_label("Files and directories");
        files_and_dirs.set_group(Some(&files_only));
        mode_group.add(&files_and_dirs);

        content.append(&mode_group);

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .child(&content)
            .build();

        let window_content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        window_content.append(&scrolled);

        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        actions.set_hexpand(true);
        actions.set_margin_top(12);
        actions.set_margin_bottom(12);
        actions.set_margin_start(18);
        actions.set_margin_end(18);
        actions.append(&cancel_button);

        let action_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        action_spacer.set_hexpand(true);
        actions.append(&action_spacer);
        actions.append(&add_button);
        window_content.append(&actions);

        form_window.set_child(Some(&window_content));
        form_window.set_default_widget(Some(&add_button));

        let page = self.downgrade();
        let form_window_weak = form_window.downgrade();
        let name_entry_for_add = name_entry.clone();
        add_button.connect_clicked(move |_| {
            let Some(page) = page.upgrade() else {
                return;
            };
            let name = name_entry_for_add.text().to_string();
            let path = path_entry.text().to_string();
            let search_name = search_entry.text().trim().to_string();
            let mode = if files_and_dirs.is_active() {
                DeletionMode::FilesAndDirectories
            } else {
                DeletionMode::FilesOnly
            };

            if name.is_empty() || path.is_empty() {
                return;
            }

            if let Some(expanded_path) = Self::expand_custom_path(&path) {
                let auditor = SecurityAuditor::new();
                let audit = auditor.audit(&expanded_path);
                if audit.is_safe {
                    if let Some(form_window) = form_window_weak.upgrade() {
                        form_window.close();
                    }
                    if search_name.is_empty() {
                        page.queue_rule_creation(
                            parent_window.as_ref(),
                            name,
                            path,
                            mode,
                            None,
                            &expanded_path,
                        );
                    } else {
                        page.show_search_preview(
                            parent_window.as_ref(),
                            name,
                            path,
                            mode,
                            search_name,
                            expanded_path,
                        );
                    }
                } else {
                    let reason = audit
                        .violations
                        .first()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "Path is not safe for cleanup".to_string());
                    let error_parent = form_window_weak.upgrade();
                    let err_dialog = adw::MessageDialog::new(
                        error_parent.as_ref(),
                        Some("Invalid Path"),
                        Some(&format!("The path '{}' is not allowed:\n{}", path, reason)),
                    );
                    err_dialog.add_response("ok", "OK");
                    err_dialog.set_default_response(Some("ok"));
                    crate::i18n::translate_widget_tree(&err_dialog);
                    err_dialog.present();
                }
            } else {
                let error_parent = form_window_weak.upgrade();
                let err_dialog = adw::MessageDialog::new(
                    error_parent.as_ref(),
                    Some("Invalid Path"),
                    Some("Could not expand home directory in path."),
                );
                err_dialog.add_response("ok", "OK");
                err_dialog.set_default_response(Some("ok"));
                crate::i18n::translate_widget_tree(&err_dialog);
                err_dialog.present();
            }
        });

        crate::i18n::translate_widget_tree(&form_window);
        form_window.present();
        name_entry.grab_focus();
    }

    fn expand_custom_path(path: &str) -> Option<PathBuf> {
        if let Some(relative) = path.strip_prefix("~/") {
            dirs::home_dir().map(|home| home.join(relative))
        } else if path == "~" {
            dirs::home_dir()
        } else {
            Some(PathBuf::from(path))
        }
    }

    fn display_custom_path(path: &std::path::Path) -> String {
        if let Some(home) = dirs::home_dir() {
            if path == home {
                return "~/".to_string();
            }
            if let Ok(relative) = path.strip_prefix(home) {
                return format!("~/{}", relative.display());
            }
        }
        path.display().to_string()
    }

    fn is_conventional_cleanup_path(path: &std::path::Path) -> bool {
        dirs::cache_dir()
            .into_iter()
            .chain(dirs::state_dir())
            .chain(std::iter::once(PathBuf::from("/tmp")))
            .any(|base| path.starts_with(&base) && path != base.as_path())
    }

    /// Route a validated rule either straight to storage (conventional
    /// cache/temp locations) or through the high-risk confirmation dialog.
    fn queue_rule_creation(
        &self,
        window: Option<&gtk4::Window>,
        name: String,
        path: String,
        mode: DeletionMode,
        subfolder_name: Option<String>,
        expanded_path: &std::path::Path,
    ) {
        if Self::is_conventional_cleanup_path(expanded_path) {
            self.add_rule(name, path, mode, subfolder_name);
        } else {
            self.confirm_high_risk_rule(window, name, path, mode, subfolder_name);
        }
    }

    /// Run the subfolder search once and preview every match before the
    /// rule is saved. The rule keeps rescanning on each cleanup run, so
    /// folders created later are still picked up.
    fn show_search_preview(
        &self,
        window: Option<&gtk4::Window>,
        name: String,
        path: String,
        mode: DeletionMode,
        search_name: String,
        root: PathBuf,
    ) {
        let scanner = crate::services::Scanner::new();

        if crate::services::Scanner::parse_subfolder_pattern(&search_name).is_none() {
            let err_dialog = adw::MessageDialog::new(
                window,
                Some("Invalid Folder Pattern"),
                Some(&format!(
                    "'{}' is not a valid folder search pattern. Use a folder name such as 'target', a relative path such as 'target/debug', without '.' or '..'.",
                    search_name
                )),
            );
            err_dialog.add_response("ok", "OK");
            err_dialog.set_default_response(Some("ok"));
            crate::i18n::translate_widget_tree(&err_dialog);
            err_dialog.present();
            return;
        }

        let matches = scanner.find_matching_subdirectories(&root, &search_name);

        let heading = if matches.is_empty() {
            "No Matches Yet".to_string()
        } else {
            format!("Found {} Match{}", matches.len(), if matches.len() == 1 { "" } else { "es" })
        };
        let body = if matches.is_empty() {
            format!(
                "No folders matching \"{}\" exist under {} right now. The rule will rescan automatically on every cleanup run and will clean any that appear later.",
                search_name, path
            )
        } else {
            format!(
                "Folders matching \"{}\" under {} will be added to the cleanup list:",
                search_name, path
            )
        };

        let dialog = adw::MessageDialog::new(window, Some(&heading), Some(&body));

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);

        const PREVIEW_LIMIT: usize = 20;
        for match_path in matches.iter().take(PREVIEW_LIMIT) {
            let row_label = gtk4::Label::new(Some(&match_path.display().to_string()));
            row_label.add_css_class("caption");
            row_label.add_css_class("monospace");
            row_label.set_ellipsize(gtk4::pango::EllipsizeMode::Start);
            row_label.set_halign(gtk4::Align::Start);
            content.append(&row_label);
        }
        if matches.len() > PREVIEW_LIMIT {
            let more_label =
                gtk4::Label::new(Some(&format!("\u{2026}and {} more", matches.len() - PREVIEW_LIMIT)));
            more_label.add_css_class("dim-label");
            more_label.add_css_class("caption");
            more_label.set_halign(gtk4::Align::Start);
            content.append(&more_label);
        }

        if !matches.is_empty() {
            dialog.set_extra_child(Some(&content));
        }

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("add", "Add Rule");
        dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("add"));
        dialog.set_close_response("cancel");

        let page = self.downgrade();
        let window_owned = window.cloned();
        dialog.connect_response(None, move |_, response| {
            if response == "add" {
                if let Some(page) = page.upgrade() {
                    page.queue_rule_creation(
                        window_owned.as_ref(),
                        name.clone(),
                        path.clone(),
                        mode,
                        Some(search_name.clone()),
                        &root,
                    );
                }
            }
        });
        crate::i18n::translate_widget_tree(&dialog);
        dialog.present();
    }

    fn confirm_high_risk_rule(
        &self,
        window: Option<&gtk4::Window>,
        name: String,
        path: String,
        mode: DeletionMode,
        subfolder_name: Option<String>,
    ) {
        let scope_note = match &subfolder_name {
            Some(search_name) => format!(
                "The rule searches it for folders matching \"{}\" and only those folders are cleaned.",
                search_name
            ),
            None => String::new(),
        };
        let dialog = adw::MessageDialog::new(
            window,
            Some("High-Risk Custom Location"),
            Some(&format!(
                "{} is outside the normal cache and temporary-data locations. It may contain projects, documents, virtual machines, or other personal data. {}\n\nThe rule will be added disabled and must be enabled manually.",
                path, scope_note
            )),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("add", "Add Disabled Rule");
        dialog.set_response_appearance("add", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let page = self.downgrade();
        dialog.connect_response(None, move |_, response| {
            if response == "add" {
                if let Some(page) = page.upgrade() {
                    page.add_rule(name.clone(), path.clone(), mode, subfolder_name.clone());
                }
            }
        });
        crate::i18n::translate_widget_tree(&dialog);
        dialog.present();
    }

    fn add_rule(
        &self,
        name: String,
        path: String,
        mode: DeletionMode,
        subfolder_name: Option<String>,
    ) {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();

        let mut rule = CustomRule::new(name, PathBuf::from(path));
        rule.deletion_mode = mode;
        rule.subfolder_name = subfolder_name.filter(|s| !s.trim().is_empty());

        if let Err(e) = storage.update_custom_rules(|rules| {
            rules.push(rule);
        }) {
            tracing::warn!("Failed to add custom rule: {}", e);
        }

        // Refresh UI
        self.refresh();
    }

    fn delete_rule(&self, rule_id: uuid::Uuid) {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();

        if let Err(e) = storage.update_custom_rules(|rules| {
            rules.retain(|r| r.id != rule_id);
        }) {
            tracing::warn!("Failed to delete custom rule: {}", e);
        }

        // Refresh UI
        self.refresh();
    }

    fn refresh(&self) {
        // Simple refresh: rebuild the whole page
        while let Some(child) = self.first_child() {
            self.remove(&child);
        }
        self.setup_ui();
    }
}
