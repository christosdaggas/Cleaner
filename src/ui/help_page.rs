// Cleaner - Help Page
// Copyright (C) 2026 Christos A. Daggas
// SPDX-License-Identifier: GPL-3.0-or-later

//! Help Page - Application documentation and guidance.

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::glib;
use gtk4::subclass::prelude::*;
use crate::i18n::tr;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct HelpPage {}

    #[glib::object_subclass]
    impl ObjectSubclass for HelpPage {
        const NAME: &'static str = "HelpPage";
        type Type = super::HelpPage;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for HelpPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_ui();
        }
    }

    impl WidgetImpl for HelpPage {}
    impl BoxImpl for HelpPage {}
}

glib::wrapper! {
    pub struct HelpPage(ObjectSubclass<imp::HelpPage>)
        @extends gtk::Widget, gtk::Box;
}

impl HelpPage {
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

        let title = gtk::Label::new(Some(&tr("Help")));
        title.add_css_class("title-1");
        title.set_halign(gtk::Align::Start);
        header_box.append(&title);

        let subtitle = gtk::Label::new(Some(&tr("Learn how to use Data Cleaner")));
        subtitle.add_css_class("dim-label");
        subtitle.set_halign(gtk::Align::Start);
        header_box.append(&subtitle);

        self.append(&header_box);

        // Scrollable content
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_hexpand(true);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 24);
        content_box.set_margin_start(24);
        content_box.set_margin_end(24);
        content_box.set_margin_top(12);
        content_box.set_margin_bottom(24);

        // About section
        content_box.append(&self.create_section(
            "About Data Cleaner",
            "Data Cleaner is a system cleaning utility for Linux that helps you free up disk space \
             by removing temporary files, caches, and other unnecessary data. It provides safe, \
             targeted cleaning for browsers, applications, and system directories while protecting \
             your important files."
        ));

        // Dashboard section
        content_box.append(&self.create_section(
            "Dashboard",
            "The Dashboard provides an overview of your system's disk usage and cleaning opportunities. \
             View total space that can be freed, see a breakdown by category, and initiate a full \
             system scan. The dashboard shows recent cleaning history and quick access to common \
             cleaning tasks."
        ));

        content_box.append(&self.create_section(
            "System Tray",
            "Data Cleaner remains available from the system tray when its window is closed. Left-click the \
             symbolic tray icon to reopen Data Cleaner, or use its menu to start a fresh scan and cleanup. \
             The Clean action follows your normal cleanup-confirmation setting. Use Close in the tray \
             menu to exit the application. GNOME requires the AppIndicator and KStatusNotifierItem Support \
             extension; KDE supports the tray icon natively."
        ));

        // Browsers section
        content_box.append(&self.create_section(
            "Browsers",
            "The Browsers page manages cache and data from web browsers installed on your system. \
             Supported browsers include Firefox, Chrome, Chromium, and others. You can selectively \
             clean:\n\n\
             • Browser cache (images, scripts, stylesheets)\n\
             • Cookies and site data\n\
             • Download history\n\
             • Browsing history\n\n\
             Be careful with cookies if you want to stay logged into websites."
        ));

        // Applications section
        content_box.append(&self.create_section(
            "Applications",
            "The Applications page handles cache and temporary files from installed applications. \
             Many applications store cached data that can be safely removed to free space. \
             The cleaner identifies application-specific cache directories and shows how much \
             space each application is using. Select which applications to clean and preserve \
             those you want to keep cached for performance."
        ));

        // Custom Directories section
        content_box.append(&self.create_section(
            "Custom Directories",
            "Add your own directories to the cleaning routine. This is useful for:\n\n\
             • Project build directories (node_modules, target, build)\n\
             • Download folders you want to clean regularly\n\
             • Temporary work directories\n\
             • Log directories\n\n\
             Configure patterns to match specific file types or exclude certain files \
             from cleaning. Custom directories are scanned during system scans."
        ));

        // Storage Analyzer section
        content_box.append(&self.create_section(
            "Storage Analyzer",
            "Choose a local folder to find the files and subfolders using the most space. \
             The treemap gives you a visual size comparison, while the list lets you browse \
             and select individual items. Selected items are moved to the system Trash after \
             confirmation, so they can be restored if needed. Storage Analyzer runs only when \
             you start it and is never included in Dashboard or scheduled cleanup."
        ));

        // System section
        content_box.append(&self.create_section(
            "System",
            "The System page handles system-level temporary files and caches. This includes:\n\n\
             • System temporary files (/tmp, /var/tmp)\n\
             • Package manager cache (dnf, apt, pacman)\n\
             • Thumbnail cache\n\
             • Old log files\n\
             • Crash reports\n\n\
             Some operations may require elevated privileges and will prompt for authentication."
        ));

        // Settings section
        content_box.append(&self.create_section(
            "Settings",
            "Configure how Cleaner operates. Set default cleaning options, configure \
             which categories to include in quick scans, manage custom directory rules, \
             and set up scheduled cleaning. You can also configure safety options \
             like file age thresholds and exclusion patterns."
        ));

        // Tips section
        content_box.append(&self.create_section(
            "Tips",
            "• Always review what will be deleted before cleaning.\n\
             • Browser cookies keep you logged into websites - clean selectively.\n\
             • Package manager caches can be useful for offline reinstalls.\n\
             • Add development build directories as custom locations to save space.\n\
             • System temporary files older than a week are usually safe to remove.\n\
             • Run a scan before cleaning to see exactly what will be removed."
        ));

        scroll.set_child(Some(&content_box));
        self.append(&scroll);
    }

    fn create_section(&self, title: &str, description: &str) -> gtk::Box {
        let section = gtk::Box::new(gtk::Orientation::Vertical, 8);

        let title_label = gtk::Label::new(Some(&tr(title)));
        title_label.add_css_class("title-3");
        title_label.set_halign(gtk::Align::Start);
        section.append(&title_label);

        let desc_label = gtk::Label::new(Some(&tr(description)));
        desc_label.set_wrap(true);
        desc_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        desc_label.set_xalign(0.0);
        desc_label.set_halign(gtk::Align::Start);
        desc_label.add_css_class("body");
        section.append(&desc_label);

        section
    }
}
