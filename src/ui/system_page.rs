use crate::models::SystemRuleType;
use crate::services::{build_plan, detect_manager, removal_command};
use crate::storage::Storage;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::glib;
use gtk4::gio;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct SystemPage {
        pub storage: RefCell<Option<Arc<Storage>>>,
        pub operation_running: Cell<bool>,
        /// How many kernel versions the user wants preserved. Read by
        /// `clean_old_kernels`; the running and newest kernels are protected on
        /// top of this regardless of the value.
        pub kernels_to_keep: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SystemPage {
        const NAME: &'static str = "DataCleanerSystemPage";
        type Type = super::SystemPage;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for SystemPage {}

    impl WidgetImpl for SystemPage {}
    impl BoxImpl for SystemPage {}
}

glib::wrapper! {
    pub struct SystemPage(ObjectSubclass<imp::SystemPage>)
        @extends gtk4::Widget, gtk4::Box;
}

impl SystemPage {
    pub fn new(storage: Arc<Storage>) -> Self {
        let page: Self = glib::Object::builder()
            .property("orientation", gtk4::Orientation::Vertical)
            .property("spacing", 0)
            .build();

        page.imp().storage.replace(Some(storage));
        // Two kernels is the distribution default almost everywhere: the one
        // you booted and the one you fall back to.
        page.imp().kernels_to_keep.set(2);
        page.setup_ui();
        page
    }

    fn setup_ui(&self) {
        let scrolled = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .build();
        self.append(&scrolled);

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

        let header = self.create_header();
        content.append(&header);

        let user_group = self.create_user_rules();
        content.append(&user_group);

        let system_group = self.create_system_rules();
        content.append(&system_group);
    }

    fn create_header(&self) -> gtk4::Widget {
        let header_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);

        let title = gtk4::Label::new(Some("System Cleanup"));
        title.add_css_class("title-2");
        title.set_halign(gtk4::Align::Start);

        let description = gtk4::Label::new(Some(
            "Clean system-level temporary files and caches. Some options require administrator privileges.",
        ));
        description.add_css_class("dim-label");
        description.set_halign(gtk4::Align::Start);
        description.set_wrap(true);
        description.set_xalign(0.0);

        header_box.append(&title);
        header_box.append(&description);

        header_box.upcast()
    }

    fn create_user_rules(&self) -> gtk4::Widget {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();
        let rules = storage.get_system_rules();

        let group = adw::PreferencesGroup::new();
        group.set_title("User-Level Cleanup");
        group.set_description(Some("These operations don't require root access"));

        for rule_type in SystemRuleType::all() {
            if !rule_type.requires_root() {
                let rule = rules.iter().find(|r| r.rule_type == *rule_type);
                let is_enabled = rule.map(|r| r.enabled).unwrap_or(false);

                let row = adw::ActionRow::new();
                row.set_title(rule_type.display_name());
                row.set_subtitle(rule_type.description());

                // Icon
                let icon = gtk4::Image::from_icon_name(rule_type.icon_name());
                icon.set_pixel_size(24);
                row.add_prefix(&icon);

                // Switch
                let switch = gtk4::Switch::new();
                switch.set_active(is_enabled);
                switch.set_valign(gtk4::Align::Center);

                let storage_clone = storage.clone();
                let rt = *rule_type;
                switch.connect_active_notify(move |sw| {
                    let enabled = sw.is_active();
                    if let Err(e) = storage_clone.update_system_rules(|rules| {
                        if let Some(rule) = rules.iter_mut().find(|r| r.rule_type == rt) {
                            rule.enabled = enabled;
                        }
                    }) {
                        tracing::warn!("Failed to update system rules: {}", e);
                    }
                });

                row.add_suffix(&switch);
                row.set_activatable_widget(Some(&switch));

                group.add(&row);
            }
        }

        group.upcast()
    }

    fn create_system_rules(&self) -> gtk4::Widget {
        let group = adw::PreferencesGroup::new();
        group.set_title("System-Level Cleanup");
        group.set_description(Some("These operations require root access"));

        // Kernel removal is done by explicit package name, never via a blanket
        // `autoremove`: autoremove takes out every package nothing currently
        // depends on and has no concept of a kernel count. See
        // services/kernel_cleanup.rs.
        let kernels_expander = adw::ExpanderRow::new();
        kernels_expander.set_title("Old Kernels");
        kernels_expander.set_subtitle("Remove superseded kernel versions");
        let kernel_icon = gtk4::Image::from_icon_name("computer-symbolic");
        kernel_icon.set_pixel_size(24);
        kernels_expander.add_prefix(&kernel_icon);

        let kernels_spin_row = adw::ActionRow::new();
        kernels_spin_row.set_title("Kernels to Keep");
        kernels_spin_row.set_subtitle("The running and newest kernels are always kept");

        let kernels_spin = gtk4::SpinButton::with_range(1.0, 10.0, 1.0);
        kernels_spin.set_digits(0);
        kernels_spin.set_value(self.imp().kernels_to_keep.get() as f64);
        kernels_spin.set_valign(gtk4::Align::Center);
        let page = self.clone();
        kernels_spin.connect_value_changed(move |spin| {
            page.imp().kernels_to_keep.set(spin.value() as u32);
        });
        kernels_spin_row.add_suffix(&kernels_spin);
        kernels_expander.add_row(&kernels_spin_row);

        let clean_kernels_row = adw::ActionRow::new();
        clean_kernels_row.set_title("Clean Old Kernels");
        clean_kernels_row.set_subtitle("Lists exactly what will be removed before asking (requires pkexec)");

        let clean_kernels_btn = gtk4::Button::with_label("Clean");
        clean_kernels_btn.add_css_class("suggested-action");
        clean_kernels_btn.set_valign(gtk4::Align::Center);
        let page = self.clone();
        clean_kernels_btn.connect_clicked(move |btn| {
            page.clean_old_kernels(btn);
        });
        clean_kernels_row.add_suffix(&clean_kernels_btn);
        kernels_expander.add_row(&clean_kernels_row);

        group.add(&kernels_expander);

        let cache_expander = adw::ExpanderRow::new();
        cache_expander.set_title("Package Manager Cache");
        cache_expander.set_subtitle("Clean downloaded package files");

        let cache_icon = gtk4::Image::from_icon_name("package-x-generic-symbolic");
        cache_icon.set_pixel_size(24);
        cache_expander.add_prefix(&cache_icon);

        let clean_apt_row = adw::ActionRow::new();
        clean_apt_row.set_title("Clean APT Cache");
        clean_apt_row.set_subtitle("Clears /var/cache/apt/archives (Debian/Ubuntu)");

        let clean_apt_btn = gtk4::Button::with_label("Clean");
        clean_apt_btn.add_css_class("suggested-action");
        clean_apt_btn.set_valign(gtk4::Align::Center);

        let page_clone = self.downgrade();
        clean_apt_btn.connect_clicked(move |btn| {
            if let Some(page) = page_clone.upgrade() {
                page.clean_apt_cache(btn);
            }
        });

        clean_apt_row.add_suffix(&clean_apt_btn);
        cache_expander.add_row(&clean_apt_row);

        let clean_dnf_row = adw::ActionRow::new();
        clean_dnf_row.set_title("Clean DNF Cache");
        clean_dnf_row.set_subtitle("Clears /var/cache/dnf (Fedora/RHEL)");

        let clean_dnf_btn = gtk4::Button::with_label("Clean");
        clean_dnf_btn.add_css_class("suggested-action");
        clean_dnf_btn.set_valign(gtk4::Align::Center);

        let page_clone = self.downgrade();
        clean_dnf_btn.connect_clicked(move |btn| {
            if let Some(page) = page_clone.upgrade() {
                page.clean_dnf_cache(btn);
            }
        });

        clean_dnf_row.add_suffix(&clean_dnf_btn);
        cache_expander.add_row(&clean_dnf_row);

        let clean_pacman_row = adw::ActionRow::new();
        clean_pacman_row.set_title("Clean Pacman Cache");
        clean_pacman_row.set_subtitle("Clears /var/cache/pacman/pkg (Arch)");

        let clean_pacman_btn = gtk4::Button::with_label("Clean");
        clean_pacman_btn.add_css_class("suggested-action");
        clean_pacman_btn.set_valign(gtk4::Align::Center);

        let page_clone = self.downgrade();
        clean_pacman_btn.connect_clicked(move |btn| {
            if let Some(page) = page_clone.upgrade() {
                page.clean_pacman_cache(btn);
            }
        });

        clean_pacman_row.add_suffix(&clean_pacman_btn);
        cache_expander.add_row(&clean_pacman_row);

        let clean_zypper_row = adw::ActionRow::new();
        clean_zypper_row.set_title("Clean Zypper Cache");
        clean_zypper_row.set_subtitle("Clears package metadata and cached packages (openSUSE)");

        let clean_zypper_btn = gtk4::Button::with_label("Clean");
        clean_zypper_btn.add_css_class("suggested-action");
        clean_zypper_btn.set_valign(gtk4::Align::Center);

        let page_clone = self.downgrade();
        clean_zypper_btn.connect_clicked(move |btn| {
            if let Some(page) = page_clone.upgrade() {
                page.clean_zypper_cache(btn);
            }
        });

        clean_zypper_row.add_suffix(&clean_zypper_btn);
        cache_expander.add_row(&clean_zypper_row);

        group.add(&cache_expander);

        let pkexec_available = find_system_command("pkexec").is_some();
        clean_kernels_btn.set_sensitive(
            pkexec_available && detect_manager().is_some(),
        );
        clean_apt_btn.set_sensitive(pkexec_available && find_system_command("apt-get").is_some());
        clean_dnf_btn.set_sensitive(pkexec_available && find_system_command("dnf").is_some());
        clean_pacman_btn.set_sensitive(pkexec_available && find_system_command("pacman").is_some());
        clean_zypper_btn.set_sensitive(pkexec_available && find_system_command("zypper").is_some());

        group.upcast()
    }

    fn request_root_command(
        &self,
        button: &gtk4::Button,
        title: &str,
        description: &str,
        args: Vec<String>,
    ) {
        if self.operation_is_running() {
            self.show_operation_running_dialog();
            return;
        }
        if let Some(window) = self
            .root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok())
            .and_then(|window| window.downcast::<crate::ui::MainWindow>().ok())
        {
            if window.dashboard_operation_is_running()
                || window.storage_analyzer_operation_is_running()
            {
                window.show_operation_running_dialog();
                return;
            }
        }

        let Some(command_name) = args.first() else {
            return;
        };
        let Some(pkexec) = find_system_command("pkexec") else {
            self.show_command_result(title, "The system pkexec executable was not found.", "");
            return;
        };
        let Some(command) = find_system_command(command_name) else {
            self.show_command_result(title, "The package-manager executable was not found.", "");
            return;
        };

        let mut command_line = vec![OsString::from(pkexec), OsString::from(command)];
        command_line.extend(args.into_iter().skip(1).map(OsString::from));
        self.imp().operation_running.set(true);

        let window = self.root().and_then(|root| root.downcast::<gtk4::Window>().ok());
        let dialog = adw::MessageDialog::new(
            window.as_ref(),
            Some(&crate::i18n::tr("Confirm Administrator Operation")),
            Some(&crate::i18n::tr_args(
                "{description}\n\nAdministrator approval is required. Review this action before continuing.",
                &[("{description}", &crate::i18n::tr(description))],
            )),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("run", "Continue");
        dialog.set_response_appearance("run", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let page = self.downgrade();
        let button = button.clone();
        let title = title.to_string();
        dialog.connect_response(None, move |_, response| {
            if let Some(page) = page.upgrade() {
                if response == "run" {
                    page.execute_root_command(&button, &title, command_line.clone());
                } else {
                    page.imp().operation_running.set(false);
                }
            }
        });
        crate::i18n::translate_widget_tree(&dialog);
        dialog.present();
    }

    fn execute_root_command(
        &self,
        button: &gtk4::Button,
        title: &str,
        cmd_args: Vec<OsString>,
    ) {
        button.set_sensitive(false);
        let original_label = button.label().map(|s| s.to_string()).unwrap_or_default();
        button.set_label("Cleaning...");

        let btn = button.clone();
        let label = original_label.clone();
        let page = self.clone();
        let command_title = title.to_string();

        glib::spawn_future_local(async move {
            let cmd_refs: Vec<&OsStr> = cmd_args.iter().map(|s| s.as_os_str()).collect();

            match gio::Subprocess::newv(
                &cmd_refs,
                gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
            ) {
                Ok(subprocess) => {
                    let (stdout, stderr, success) = match subprocess.communicate_utf8_future(None).await {
                        Ok((stdout, stderr)) => (
                            stdout.unwrap_or_default().to_string(),
                            stderr.unwrap_or_default().to_string(),
                            subprocess.wait_check_future().await.is_ok(),
                        ),
                        Err(err) => (String::new(), err.to_string(), false),
                    };

                    btn.set_sensitive(true);
                    page.imp().operation_running.set(false);
                    if success {
                        btn.set_label("Done");
                    } else {
                        btn.set_label("Failed");
                        page.show_command_result(
                            &command_title,
                            stderr.trim(),
                            stdout.trim(),
                        );
                    }

                    let btn2 = btn.clone();
                    let label2 = label.clone();
                    glib::timeout_add_seconds_local_once(3, move || {
                        btn2.set_label(&label2);
                    });
                }
                Err(err) => {
                    btn.set_sensitive(true);
                    btn.set_label(&label);
                    page.imp().operation_running.set(false);
                    page.show_command_result(&command_title, &err.to_string(), "");
                }
            }
        });
    }

    /// Build a removal plan, show the user exactly which packages it covers,
    /// and only then ask for administrator approval. Nothing is executed if the
    /// plan is empty — several package managers treat an empty package list as
    /// "operate on everything".
    fn clean_old_kernels(&self, button: &gtk4::Button) {
        let keep = self.imp().kernels_to_keep.get();
        let plan = match build_plan(keep) {
            Ok(plan) => plan,
            Err(error) => {
                self.show_command_result("Kernel Cleanup Failed", &error, "");
                return;
            }
        };

        let Some(args) = removal_command(&plan) else {
            self.show_command_result("Nothing to Remove", &plan.summary(), "");
            return;
        };

        self.request_root_command(button, "Kernel Cleanup Failed", &plan.summary(), args);
    }

    fn clean_apt_cache(&self, button: &gtk4::Button) {
        self.request_root_command(
            button,
            "APT Cleanup Failed",
            "Delete downloaded package archives from the APT cache. No installed packages will be removed.",
            vec!["apt-get".to_string(), "clean".to_string()],
        );
    }

    fn clean_dnf_cache(&self, button: &gtk4::Button) {
        self.request_root_command(
            button,
            "DNF Cleanup Failed",
            "Delete cached DNF packages and metadata. No installed packages will be removed.",
            vec!["dnf".to_string(), "clean".to_string(), "all".to_string()],
        );
    }

    fn clean_pacman_cache(&self, button: &gtk4::Button) {
        self.request_root_command(
            button,
            "Pacman Cleanup Failed",
            "Delete package versions that are no longer installed from the Pacman cache. No installed packages will be removed.",
            vec!["pacman".to_string(), "-Sc".to_string(), "--noconfirm".to_string()],
        );
    }

    fn clean_zypper_cache(&self, button: &gtk4::Button) {
        self.request_root_command(
            button,
            "Zypper Cleanup Failed",
            "Delete cached Zypper packages and metadata. No installed packages will be removed.",
            vec![
                "zypper".to_string(),
                "--non-interactive".to_string(),
                "clean".to_string(),
                "--all".to_string(),
            ],
        );
    }

    pub fn operation_is_running(&self) -> bool {
        self.imp().operation_running.get()
    }

    pub fn show_operation_running_dialog(&self) {
        self.show_command_result(
            "System Operation in Progress",
            "Wait for the current administrator operation to finish before starting another operation or closing Data Cleaner.",
            "",
        );
    }

    fn show_command_result(&self, title: &str, stderr: &str, stdout: &str) {
        let mut message = String::new();
        if !stderr.is_empty() {
            message.push_str(stderr);
        } else if !stdout.is_empty() {
            message.push_str(stdout);
        } else {
            message.push_str("The privileged command failed without additional output.");
        }

        let window = self.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
        let dialog = adw::MessageDialog::new(
            window.as_ref(),
            Some(&crate::i18n::tr(title)),
            Some(&message),
        );

        dialog.add_response("ok", "OK");
        dialog.set_default_response(Some("ok"));
        crate::i18n::translate_widget_tree(&dialog);
        dialog.present();
    }
}

fn find_system_command(command: &str) -> Option<PathBuf> {
    ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
        .iter()
        .map(|directory| Path::new(directory).join(command))
        .find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        path.metadata()
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        true
    }
}
