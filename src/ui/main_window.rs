use crate::storage::Storage;
use crate::theme;
use crate::ui::{
    ApplicationsPage, BrowsersPage, CleanupLogPage, CustomPage, DashboardPage, DiagnosticsPage,
    HelpPage, SettingsPage, StorageAnalyzerPage, SystemPage,
};
use crate::version_check;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

/// Navigation items for the sidebar
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    Dashboard,
    Browsers,
    Applications,
    CustomDirectories,
    StorageAnalyzer,
    System,
    Settings,
    Diagnostics,
    Help,
}

impl NavItem {
    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::Dashboard => "view-grid-symbolic",
            Self::Browsers => "web-browser-symbolic",
            Self::Applications => "application-x-executable-symbolic",
            Self::CustomDirectories => "folder-symbolic",
            Self::StorageAnalyzer => "drive-harddisk-symbolic",
            Self::System => "computer-symbolic",
            Self::Settings => "preferences-system-symbolic",
            Self::Diagnostics => "dialog-information-symbolic",
            Self::Help => "help-about-symbolic",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Browsers => "Browsers",
            Self::Applications => "Applications",
            Self::CustomDirectories => "Custom Directories",
            Self::StorageAnalyzer => "Storage Analyzer",
            Self::System => "System",
            Self::Settings => "Settings",
            Self::Diagnostics => "Diagnostics",
            Self::Help => "Help",
        }
    }

    pub fn stack_name(&self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Browsers => "browsers",
            Self::Applications => "applications",
            Self::CustomDirectories => "custom",
            Self::StorageAnalyzer => "storage-analyzer",
            Self::System => "system",
            Self::Settings => "settings",
            Self::Diagnostics => "diagnostics",
            Self::Help => "help",
        }
    }

    pub fn all() -> &'static [NavItem] {
        &[
            Self::Dashboard,
            Self::Browsers,
            Self::Applications,
            Self::CustomDirectories,
            Self::StorageAnalyzer,
            Self::System,
            Self::Settings,
            Self::Diagnostics,
            Self::Help,
        ]
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct MainWindow {
        pub storage: RefCell<Option<Arc<Storage>>>,
        pub sidebar_box: RefCell<Option<gtk4::Box>>,
        pub content_stack: RefCell<Option<gtk4::Stack>>,
        pub sidebar_list: RefCell<Option<gtk4::ListBox>>,
        pub dashboard_page: RefCell<Option<DashboardPage>>,
        pub cleanup_log_page: RefCell<Option<CleanupLogPage>>,
        pub storage_analyzer_page: RefCell<Option<StorageAnalyzerPage>>,
        pub system_page: RefCell<Option<SystemPage>>,
        pub content_title: RefCell<Option<adw::WindowTitle>>,
        pub update_button: RefCell<Option<gtk4::LinkButton>>,
        pub update_label: RefCell<Option<gtk4::Label>>,
        // Sidebar collapse state
        pub sidebar_collapsed: Cell<bool>,
        pub sidebar_toggle_btn: RefCell<Option<gtk4::Button>>,
        pub sidebar_title: RefCell<Option<adw::WindowTitle>>,
        pub nav_labels: RefCell<Vec<gtk4::Label>>,
        pub nav_boxes: RefCell<Vec<gtk4::Box>>,
        pub update_check_started: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MainWindow {
        const NAME: &'static str = "DataCleanerMainWindow";
        type Type = super::MainWindow;
        type ParentType = adw::ApplicationWindow;
    }

    impl ObjectImpl for MainWindow {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for MainWindow {}
    impl WindowImpl for MainWindow {}
    impl ApplicationWindowImpl for MainWindow {}
    impl AdwApplicationWindowImpl for MainWindow {}
}

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends gtk4::Widget, gtk4::Window, gtk4::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl MainWindow {
    /// Sidebar widths, matched to the Speech to Text app so both applications
    /// present the same navigation rail in either state.
    ///
    /// These are the widths STT actually *renders*, measured from a screenshot,
    /// not the constants in its source: STT asks for 50px but its sidebar
    /// header's natural width floors the collapsed rail at 94px. Cleaner's
    /// header is a plain box with no such floor, so the rail lands exactly on
    /// the value below.
    const SIDEBAR_EXPANDED_WIDTH: i32 = 260;
    const SIDEBAR_COLLAPSED_WIDTH: i32 = 80;

    pub fn new(app: &crate::application::DataCleanerApplication, storage: Arc<Storage>) -> Self {
        let window: Self = glib::Object::builder()
            .property("application", app)
            .property("title", crate::APP_NAME)
            .property("default-width", 1060)
            .property("default-height", 680)
            .build();

        window.imp().storage.replace(Some(storage));
        window.setup_ui();
        window.setup_actions();
        window.sync_theme_preferences();
        window
    }

    fn setup_ui(&self) {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();

        // Main horizontal layout: a dark navigation rail and a lighter,
        // uninterrupted content surface.
        let main_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        main_box.add_css_class("background");
        main_box.add_css_class("data-cleaner-window-layout");

        // === SIDEBAR ===
        let sidebar_box = self.create_sidebar(&storage);

        // === CONTENT AREA ===
        let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content_box.set_hexpand(true);
        content_box.add_css_class("data-cleaner-content");
        content_box.add_css_class("data-cleaner-view");
        content_box.add_css_class("view");

        // Header bar for content area
        let header = adw::HeaderBar::new();
        header.add_css_class("toolbar");
        header.add_css_class("data-cleaner-content-header");
        let title = adw::WindowTitle::new("Dashboard", "");
        title.set_visible(false);
        header.set_title_widget(Some(&title));

        // Add menu button (hamburger menu)
        let menu_button = gtk4::MenuButton::new();
        menu_button.set_icon_name("open-menu-symbolic");
        menu_button.set_tooltip_text(Some("Main Menu"));

        // Create custom popover with theme selector
        let popover = Self::create_main_menu_popover();
        menu_button.set_popover(Some(&popover));
        header.pack_end(&menu_button);

        content_box.append(&header);

        // Content stack
        let content_stack = gtk4::Stack::new();
        content_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        content_stack.set_transition_duration(theme::transition_duration(self, 200));
        content_stack.set_vexpand(true);
        content_stack.set_hexpand(true);
        content_stack.add_css_class("view");
        content_stack.add_css_class("data-cleaner-view");

        // Create pages
        let dashboard_page = DashboardPage::new(storage.clone());
        let cleanup_log_page = CleanupLogPage::new();
        let browsers_page = BrowsersPage::new(storage.clone());
        let applications_page = ApplicationsPage::new(storage.clone());
        let custom_page = CustomPage::new(storage.clone());
        let storage_analyzer_page = StorageAnalyzerPage::new();
        let system_page = SystemPage::new(storage.clone());
        let settings_page = SettingsPage::new(storage.clone());
        let diagnostics_page = DiagnosticsPage::new();
        let help_page = HelpPage::new();

        // Add pages to stack
        content_stack.add_named(&dashboard_page, Some(NavItem::Dashboard.stack_name()));
        // This page is deliberately absent from NavItem::all(), so it can be
        // opened after a cleanup without appearing in the sidebar.
        content_stack.add_named(&cleanup_log_page, Some("cleanup-log"));
        content_stack.add_named(&browsers_page, Some(NavItem::Browsers.stack_name()));
        content_stack.add_named(&applications_page, Some(NavItem::Applications.stack_name()));
        content_stack.add_named(&custom_page, Some(NavItem::CustomDirectories.stack_name()));
        content_stack.add_named(
            &storage_analyzer_page,
            Some(NavItem::StorageAnalyzer.stack_name()),
        );
        content_stack.add_named(&system_page, Some(NavItem::System.stack_name()));
        content_stack.add_named(&settings_page, Some(NavItem::Settings.stack_name()));
        content_stack.add_named(&diagnostics_page, Some(NavItem::Diagnostics.stack_name()));
        content_stack.add_named(&help_page, Some(NavItem::Help.stack_name()));

        content_box.append(&content_stack);
        content_box.append(&self.create_status_bar());

        // Assemble main layout
        main_box.append(&sidebar_box);
        main_box.append(&content_box);

        // Store references
        self.imp().sidebar_box.replace(Some(sidebar_box));
        self.imp().content_stack.replace(Some(content_stack.clone()));
        self.imp().dashboard_page.replace(Some(dashboard_page));
        self.imp().cleanup_log_page.replace(Some(cleanup_log_page));
        self.imp()
            .storage_analyzer_page
            .replace(Some(storage_analyzer_page));
        self.imp().system_page.replace(Some(system_page));
        self.imp().content_title.replace(Some(title.clone()));

        // Handle navigation
        let sidebar_list = self.imp().sidebar_list.borrow().clone();
        if let Some(list) = sidebar_list {
            let stack = content_stack.clone();
            let title_widget = title.clone();
            let window_weak = self.downgrade();
            list.connect_row_selected(move |_, row| {
                if let Some(row) = row {
                    let index = row.index() as usize;
                    if let Some(nav_item) = NavItem::all().get(index) {
                        stack.set_visible_child_name(nav_item.stack_name());
                        title_widget.set_title(&crate::i18n::tr(nav_item.title()));
                        title_widget.set_visible(*nav_item != NavItem::Dashboard);

                        // When returning to the Dashboard, re-read storage so
                        // the enabled-targets count and the Clean button state
                        // reflect any rule toggles the user made on other
                        // pages. Without this the dashboard shows stale data
                        // and the Clean button can stay disabled.
                        if *nav_item == NavItem::Dashboard {
                            if let Some(window) = window_weak.upgrade() {
                                window.refresh_dashboard();
                            }
                        }
                    }
                }
            });
        }

        self.set_content(Some(&main_box));
        self.toggle_sidebar();
        crate::i18n::translate_widget_tree(&main_box);

        // Check GitHub once when the window is created. The request runs in
        // the background and only changes the footer when an update exists.
        self.check_for_updates();
    }

    fn create_sidebar(&self, _storage: &Arc<Storage>) -> gtk4::Box {
        let sidebar_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        sidebar_box.set_width_request(Self::SIDEBAR_EXPANDED_WIDTH);
        // hexpand propagates up from any descendant that sets it. The title
        // below expands so the collapse button sits on the trailing edge, and
        // without these the whole rail would start competing with the content
        // area for leftover width instead of staying at its requested size.
        sidebar_box.set_hexpand(false);
        sidebar_box.add_css_class("sidebar-box");
        sidebar_box.add_css_class("data-cleaner-sidebar");

        // Sidebar header.
        //
        // A plain GtkBox rather than an AdwHeaderBar: the header bar reserves
        // room for its title area even with both title-button sets disabled and
        // the title widget hidden, giving it a natural width of 104px. Since a
        // non-expanding GtkBox child is allocated its natural width, that floor
        // made the collapsed rail 104px wide no matter what `width_request`
        // asked for. A box asks for exactly what its children need, so the rail
        // can reach SIDEBAR_COLLAPSED_WIDTH.
        let sidebar_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        sidebar_header.set_height_request(47);
        sidebar_header.set_hexpand(false);
        sidebar_header.add_css_class("toolbar");
        sidebar_header.add_css_class("data-cleaner-sidebar-header");

        let sidebar_title = adw::WindowTitle::new(crate::APP_NAME, "");
        sidebar_title.set_hexpand(true);
        sidebar_title.set_halign(gtk4::Align::Start);
        sidebar_title.set_margin_start(12);
        sidebar_header.append(&sidebar_title);

        // Sidebar collapse button (trailing edge of the sidebar header)
        let sidebar_toggle_btn = gtk4::Button::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Collapse sidebar")
            .build();
        sidebar_toggle_btn.add_css_class("flat");
        sidebar_toggle_btn.set_valign(gtk4::Align::Center);
        sidebar_toggle_btn.set_action_name(Some("win.toggle-sidebar"));
        sidebar_header.append(&sidebar_toggle_btn);

        sidebar_box.append(&sidebar_header);

        // Navigation list
        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);

        let sidebar_list = gtk4::ListBox::new();
        sidebar_list.set_selection_mode(gtk4::SelectionMode::Single);
        sidebar_list.add_css_class("navigation-sidebar");

        // Add navigation items and collect labels and row hboxes so collapse
        // can hide labels AND re-center the icons.
        let mut nav_labels = Vec::new();
        let mut nav_boxes = Vec::new();
        for nav_item in NavItem::all() {
            let (row, label, hbox) = self.create_nav_row_with_label(*nav_item);
            sidebar_list.append(&row);
            nav_labels.push(label);
            nav_boxes.push(hbox);
        }

        // Select first row by default
        if let Some(first_row) = sidebar_list.row_at_index(0) {
            sidebar_list.select_row(Some(&first_row));
        }

        self.imp().sidebar_list.replace(Some(sidebar_list.clone()));

        scrolled.set_child(Some(&sidebar_list));
        sidebar_box.append(&scrolled);

        // Store references for collapse/expand
        self.imp().sidebar_toggle_btn.replace(Some(sidebar_toggle_btn));
        self.imp().sidebar_title.replace(Some(sidebar_title));
        self.imp().nav_labels.replace(nav_labels);
        self.imp().nav_boxes.replace(nav_boxes);
        self.imp().sidebar_collapsed.set(false);

        sidebar_box
    }

    fn create_status_bar(&self) -> gtk4::Box {
        let status_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        status_bar.add_css_class("app-status-bar");

        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        status_bar.append(&spacer);

        // Hidden until the startup GitHub check finds a newer release.
        let update_inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        let update_icon = gtk4::Image::from_icon_name("software-update-available-symbolic");
        update_icon.set_pixel_size(10);
        update_icon.add_css_class("error");
        update_inner.append(&update_icon);

        let update_label = gtk4::Label::new(Some("Update available"));
        update_label.add_css_class("caption");
        update_label.add_css_class("error");
        update_inner.append(&update_label);

        let update_button = gtk4::LinkButton::new(
            "https://github.com/christosdaggas/Cleaner/releases/latest",
        );
        update_button.set_child(Some(&update_inner));
        update_button.add_css_class("flat");
        update_button.add_css_class("update-indicator");
        update_button.set_visible(false);
        update_button.set_tooltip_text(Some(
            "Open the latest release on GitHub and verify its SHA-256 checksum before installing",
        ));
        status_bar.append(&update_button);

        let version_label = gtk4::Label::new(Some(&format!(
            "{} {}",
            crate::i18n::tr("Version"),
            crate::DISPLAY_VERSION
        )));
        version_label.add_css_class("app-version-label");
        status_bar.append(&version_label);

        self.imp().update_button.replace(Some(update_button));
        self.imp().update_label.replace(Some(update_label));
        status_bar
    }

    fn create_nav_row_with_label(
        &self,
        nav_item: NavItem,
    ) -> (gtk4::ListBoxRow, gtk4::Label, gtk4::Box) {
        let row = gtk4::ListBoxRow::new();
        row.set_selectable(true);
        row.set_tooltip_text(Some(nav_item.title()));

        let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        hbox.set_margin_top(16);
        hbox.set_margin_bottom(16);
        hbox.set_margin_start(12);
        hbox.set_margin_end(12);
        hbox.add_css_class("nav-row-box");

        let icon = gtk4::Image::from_icon_name(nav_item.icon_name());
        icon.set_pixel_size(20);
        hbox.append(&icon);

        let label = gtk4::Label::new(Some(nav_item.title()));
        label.set_halign(gtk4::Align::Start);
        label.set_hexpand(true);
        label.add_css_class("nav-label");
        hbox.append(&label);

        row.set_child(Some(&hbox));
        (row, label, hbox)
    }

    fn setup_actions(&self) {
        // Refresh action
        let refresh_action = gio::ActionEntry::builder("refresh")
            .activate(|window: &Self, _, _| {
                window.refresh_dashboard();
            })
            .build();

        // Toggle sidebar action
        let toggle_sidebar_action = gio::ActionEntry::builder("toggle-sidebar")
            .activate(|window: &Self, _, _| {
                window.toggle_sidebar();
            })
            .build();

        self.add_action_entries([refresh_action, toggle_sidebar_action]);
    }

    /// Toggle the sidebar between expanded and collapsed (icon-only) mode.
    fn toggle_sidebar(&self) {
        let imp = self.imp();

        let is_collapsed = imp.sidebar_collapsed.get();
        let new_collapsed = !is_collapsed;
        imp.sidebar_collapsed.set(new_collapsed);

        // Update sidebar width. `set_width_request` only raises the minimum —
        // a GtkBox child that does not hexpand is allocated its *natural*
        // width, so the collapsed rail also needs the CSS below to shrink the
        // header bar's natural width down to the request. Without it the
        // sidebar sticks at the header bar's natural 104px.
        if let Some(sidebar_box) = imp.sidebar_box.borrow().as_ref() {
            if new_collapsed {
                sidebar_box.set_width_request(Self::SIDEBAR_COLLAPSED_WIDTH);
                sidebar_box.add_css_class("sidebar-collapsed");
            } else {
                sidebar_box.set_width_request(Self::SIDEBAR_EXPANDED_WIDTH);
                sidebar_box.remove_css_class("sidebar-collapsed");
            }
        }

        // Hide/show sidebar title
        if let Some(sidebar_title) = imp.sidebar_title.borrow().as_ref() {
            sidebar_title.set_visible(!new_collapsed);
        }

        // Hide/show navigation labels
        for label in imp.nav_labels.borrow().iter() {
            label.set_visible(!new_collapsed);
        }

        // Recenter the icon-only hbox when collapsed. The expanded layout
        // pads each row with 12px on either side, but the collapsed sidebar
        // is only 40px wide; with the label hidden, those margins push the
        // icon visibly to the left. Drop the side margins and center the
        // hbox so the icon sits in the middle of the rail.
        for hbox in imp.nav_boxes.borrow().iter() {
            if new_collapsed {
                hbox.set_margin_start(0);
                hbox.set_margin_end(0);
                hbox.set_halign(gtk4::Align::Center);
            } else {
                hbox.set_margin_start(12);
                hbox.set_margin_end(12);
                hbox.set_halign(gtk4::Align::Fill);
            }
        }

        // Update toggle button tooltip and icon
        if let Some(btn) = imp.sidebar_toggle_btn.borrow().as_ref() {
            if new_collapsed {
                btn.set_tooltip_text(Some(&crate::i18n::tr("Expand sidebar")));
                btn.set_icon_name("sidebar-show-right-symbolic");
            } else {
                btn.set_tooltip_text(Some(&crate::i18n::tr("Collapse sidebar")));
                btn.set_icon_name("sidebar-show-symbolic");
            }
        }
    }

    pub fn refresh_dashboard(&self) {
        if let Some(dashboard) = self.imp().dashboard_page.borrow().as_ref() {
            dashboard.refresh();
        }
    }

    pub fn set_cleanup_log(
        &self,
        clean_result: &crate::models::CleanResult,
        scan_result: &crate::models::ScanResult,
        journal_result: Option<&Result<String, String>>,
        automatic: bool,
        system_journal_enabled: bool,
    ) {
        if let Some(page) = self.imp().cleanup_log_page.borrow().as_ref() {
            page.set_result(
                clean_result,
                scan_result,
                journal_result,
                automatic,
                system_journal_enabled,
            );
        }
    }

    pub fn navigate_to_cleanup_log(&self) {
        if let Some(stack) = self.imp().content_stack.borrow().as_ref() {
            stack.set_visible_child_name("cleanup-log");
        }
        if let Some(title) = self.imp().content_title.borrow().as_ref() {
            title.set_title(&crate::i18n::tr("Cleanup Log"));
            title.set_visible(true);
        }
        if let Some(sidebar) = self.imp().sidebar_list.borrow().as_ref() {
            sidebar.unselect_all();
        }
    }

    pub fn navigate_to_dashboard(&self) {
        if let Some(stack) = self.imp().content_stack.borrow().as_ref() {
            stack.set_visible_child_name(NavItem::Dashboard.stack_name());
        }
        if let Some(title) = self.imp().content_title.borrow().as_ref() {
            title.set_title(&crate::i18n::tr(NavItem::Dashboard.title()));
            title.set_visible(false);
        }
        if let Some(sidebar) = self.imp().sidebar_list.borrow().as_ref() {
            if let Some(row) = sidebar.row_at_index(0) {
                sidebar.select_row(Some(&row));
            }
        }
        self.refresh_dashboard();
    }

    pub fn request_clean(&self) {
        if self.storage_analyzer_operation_is_running() || self.system_operation_is_running() {
            self.show_operation_running_dialog();
            return;
        }
        if let Some(stack) = self.imp().content_stack.borrow().as_ref() {
            stack.set_visible_child_name(NavItem::Dashboard.stack_name());
        }
        if let Some(sidebar_list) = self.imp().sidebar_list.borrow().as_ref() {
            if let Some(row) = sidebar_list.row_at_index(0) {
                sidebar_list.select_row(Some(&row));
            }
        }
        if let Some(dashboard) = self.imp().dashboard_page.borrow().as_ref() {
            dashboard.request_clean();
        }
    }

    pub fn request_scheduled_clean(&self) -> bool {
        if self.storage_analyzer_operation_is_running() || self.system_operation_is_running() {
            return false;
        }
        self.imp()
            .dashboard_page
            .borrow()
            .as_ref()
            .map(DashboardPage::request_scheduled_clean)
            .unwrap_or(false)
    }

    pub fn operation_is_running(&self) -> bool {
        self.dashboard_operation_is_running()
            || self.storage_analyzer_operation_is_running()
            || self.system_operation_is_running()
    }

    pub fn dashboard_operation_is_running(&self) -> bool {
        self.imp()
            .dashboard_page
            .borrow()
            .as_ref()
            .map(DashboardPage::operation_is_running)
            .unwrap_or(false)
    }

    pub fn storage_analyzer_operation_is_running(&self) -> bool {
        self.imp()
            .storage_analyzer_page
            .borrow()
            .as_ref()
            .map(StorageAnalyzerPage::operation_is_running)
            .unwrap_or(false)
    }

    pub fn system_operation_is_running(&self) -> bool {
        self.imp()
            .system_page
            .borrow()
            .as_ref()
            .map(SystemPage::operation_is_running)
            .unwrap_or(false)
    }

    pub fn show_operation_running_dialog(&self) {
        if let Some(analyzer) = self.imp().storage_analyzer_page.borrow().as_ref() {
            if analyzer.operation_is_running() {
                analyzer.show_operation_running_dialog();
                return;
            }
        }
        if let Some(system) = self.imp().system_page.borrow().as_ref() {
            if system.operation_is_running() {
                system.show_operation_running_dialog();
                return;
            }
        }
        if let Some(dashboard) = self.imp().dashboard_page.borrow().as_ref() {
            dashboard.show_operation_running_dialog();
        }
    }

    pub fn navigate_to_settings(&self) {
        // Switch to Settings page
        if let Some(stack) = self.imp().content_stack.borrow().as_ref() {
            stack.set_visible_child_name(NavItem::Settings.stack_name());
        }

        // Update sidebar selection
        if let Some(sidebar_list) = self.imp().sidebar_list.borrow().as_ref() {
            let settings_index = NavItem::all()
                .iter()
                .position(|item| *item == NavItem::Settings)
                .unwrap_or(6) as i32;
            if let Some(row) = sidebar_list.row_at_index(settings_index) {
                sidebar_list.select_row(Some(&row));
            }
        }
    }

    pub fn storage(&self) -> Option<Arc<Storage>> {
        self.imp().storage.borrow().as_ref().cloned()
    }

    fn create_main_menu_popover() -> gtk4::Popover {
        let popover = gtk4::Popover::new();
        popover.add_css_class("menu");

        let main_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(0)
            .width_request(280)
            .build();

        // Theme selector section
        let theme_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(18)
            .halign(gtk4::Align::Center)
            .margin_top(18)
            .margin_bottom(18)
            .build();

        // Create theme toggle buttons
        let default_btn = gtk4::ToggleButton::new();
        let light_btn = gtk4::ToggleButton::new();
        let dark_btn = gtk4::ToggleButton::new();

        // Helper to create theme button content
        fn create_theme_content(css_class: &str, is_selected: bool) -> gtk4::Overlay {
            let overlay = gtk4::Overlay::new();

            let icon = gtk4::Box::builder()
                .width_request(44)
                .height_request(44)
                .build();
            icon.add_css_class("theme-selector");
            icon.add_css_class(css_class);
            overlay.set_child(Some(&icon));

            if is_selected {
                let check = gtk4::Image::from_icon_name("object-select-symbolic");
                check.add_css_class("theme-check");
                check.set_halign(gtk4::Align::Center);
                check.set_valign(gtk4::Align::Center);
                overlay.add_overlay(&check);
            }

            overlay
        }

        fn sync_theme_selector(
            default_btn: &gtk4::ToggleButton,
            light_btn: &gtk4::ToggleButton,
            dark_btn: &gtk4::ToggleButton,
        ) {
            let style_manager = adw::StyleManager::default();
            let (default_selected, light_selected, dark_selected) = match style_manager.color_scheme()
            {
                adw::ColorScheme::ForceLight | adw::ColorScheme::PreferLight => (false, true, false),
                adw::ColorScheme::ForceDark | adw::ColorScheme::PreferDark => (false, false, true),
                _ => (true, false, false),
            };

            default_btn.set_active(default_selected);
            light_btn.set_active(light_selected);
            dark_btn.set_active(dark_selected);
            default_btn.set_child(Some(&create_theme_content("theme-default", default_selected)));
            light_btn.set_child(Some(&create_theme_content("theme-light", light_selected)));
            dark_btn.set_child(Some(&create_theme_content("theme-dark", dark_selected)));
        }

        // Set initial content
        default_btn.set_tooltip_text(Some("System"));
        default_btn.add_css_class("flat");
        default_btn.add_css_class("circular");
        default_btn.add_css_class("theme-button");

        light_btn.set_tooltip_text(Some("Light"));
        light_btn.add_css_class("flat");
        light_btn.add_css_class("circular");
        light_btn.add_css_class("theme-button");

        dark_btn.set_tooltip_text(Some("Dark"));
        dark_btn.add_css_class("flat");
        dark_btn.add_css_class("circular");
        dark_btn.add_css_class("theme-button");

        // Group the toggle buttons (radio-button behavior)
        light_btn.set_group(Some(&default_btn));
        dark_btn.set_group(Some(&default_btn));

        sync_theme_selector(&default_btn, &light_btn, &dark_btn);

        // Connect theme button signals
        default_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                let style_manager = adw::StyleManager::default();
                style_manager.set_color_scheme(adw::ColorScheme::Default);
            }
        });

        light_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                let style_manager = adw::StyleManager::default();
                style_manager.set_color_scheme(adw::ColorScheme::ForceLight);
            }
        });

        dark_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                let style_manager = adw::StyleManager::default();
                style_manager.set_color_scheme(adw::ColorScheme::ForceDark);
            }
        });

        let default_btn_clone = default_btn.clone();
        let light_btn_clone = light_btn.clone();
        let dark_btn_clone = dark_btn.clone();
        adw::StyleManager::default().connect_color_scheme_notify(move |_| {
            sync_theme_selector(&default_btn_clone, &light_btn_clone, &dark_btn_clone);
        });

        theme_box.append(&default_btn);
        theme_box.append(&light_btn);
        theme_box.append(&dark_btn);
        main_box.append(&theme_box);

        // Separator
        let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        separator.set_margin_start(12);
        separator.set_margin_end(12);
        main_box.append(&separator);

        // Menu items
        let menu_list = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        menu_list.set_margin_top(6);
        menu_list.set_margin_bottom(6);
        menu_list.set_margin_start(6);
        menu_list.set_margin_end(6);

        // About button
        let about_btn = gtk4::Button::new();
        let about_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        about_box.set_margin_start(6);
        about_box.set_margin_end(6);
        about_box.set_margin_top(8);
        about_box.set_margin_bottom(8);
        let about_icon = gtk4::Image::from_icon_name("help-about-symbolic");
        let about_label = gtk4::Label::new(Some("About Data Cleaner"));
        about_label.set_halign(gtk4::Align::Start);
        about_label.set_hexpand(true);
        about_box.append(&about_icon);
        about_box.append(&about_label);
        about_btn.set_child(Some(&about_box));
        about_btn.add_css_class("flat");
        about_btn.add_css_class("menu-item");
        about_btn.set_action_name(Some("app.about"));
        menu_list.append(&about_btn);

        // What's New button
        let whats_new_btn = gtk4::Button::new();
        let whats_new_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        whats_new_box.set_margin_start(6);
        whats_new_box.set_margin_end(6);
        whats_new_box.set_margin_top(8);
        whats_new_box.set_margin_bottom(8);
        let whats_new_icon = gtk4::Image::from_icon_name("starred-symbolic");
        let whats_new_label = gtk4::Label::new(Some("What's New"));
        whats_new_label.set_halign(gtk4::Align::Start);
        whats_new_label.set_hexpand(true);
        whats_new_box.append(&whats_new_icon);
        whats_new_box.append(&whats_new_label);
        whats_new_btn.set_child(Some(&whats_new_box));
        whats_new_btn.add_css_class("flat");
        whats_new_btn.add_css_class("menu-item");
        whats_new_btn.set_action_name(Some("app.whats-new"));
        menu_list.append(&whats_new_btn);

        menu_list.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        // Quit button
        let quit_btn = gtk4::Button::new();
        let quit_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        quit_box.set_margin_start(6);
        quit_box.set_margin_end(6);
        quit_box.set_margin_top(8);
        quit_box.set_margin_bottom(8);
        let quit_icon = gtk4::Image::from_icon_name("application-exit-symbolic");
        let quit_label = gtk4::Label::new(Some("Quit"));
        quit_label.set_halign(gtk4::Align::Start);
        quit_label.set_hexpand(true);
        quit_box.append(&quit_icon);
        quit_box.append(&quit_label);
        quit_btn.set_child(Some(&quit_box));
        quit_btn.add_css_class("flat");
        quit_btn.add_css_class("menu-item");
        quit_btn.set_action_name(Some("app.quit"));
        menu_list.append(&quit_btn);

        main_box.append(&menu_list);

        crate::i18n::translate_widget_tree(&main_box);
        popover.set_child(Some(&main_box));
        popover
    }

    pub fn sync_theme_preferences(&self) {
        theme::sync_runtime_classes(self);

        if let Some(stack) = self.imp().content_stack.borrow().as_ref() {
            stack.set_transition_duration(theme::transition_duration(self, 200));
        }
    }

    /// Rebuild the page tree so changing away from an already translated
    /// language always starts from the canonical English source strings.
    pub fn reload_translations(&self) {
        if self.operation_is_running() {
            self.show_operation_running_dialog();
            return;
        }
        let was_collapsed = self.imp().sidebar_collapsed.get();
        self.setup_ui();
        if self.imp().sidebar_collapsed.get() != was_collapsed {
            self.toggle_sidebar();
        }
        self.sync_theme_preferences();
    }

    /// Run the one-time GitHub release check in the background.
    fn check_for_updates(&self) {
        if self.imp().update_check_started.replace(true) {
            return;
        }
        let obj_weak = self.downgrade();
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Spawn the HTTP request on the Tokio runtime
        crate::runtime().spawn(async move {
            let result = version_check::check_for_update(crate::VERSION).await;
            let _ = tx.send(result);
        });

        // Receive the result on the GTK main thread
        glib::spawn_future_local(async move {
            if let Ok(Some(update_info)) = rx.await {
                if let Some(obj) = obj_weak.upgrade() {
                    obj.show_update_available(&update_info);
                }
            }
        });
    }

    /// Display the compact clickable update indicator beside the version.
    fn show_update_available(&self, info: &version_check::UpdateInfo) {
        let imp = self.imp();
        if let Some(label) = imp.update_label.borrow().as_ref() {
            label.set_text(&format!("v{} available", info.latest_version));
        }
        if let Some(button) = imp.update_button.borrow().as_ref() {
            button.set_uri(&info.download_url);
            button.set_visible(true);
        }
        tracing::info!("Update available: v{}", info.latest_version);
    }
}
