use crate::services::{
    analyze_storage, move_to_trash, StorageAnalysis, StorageAnalysisError, StorageNode,
    StorageScanOptions, TrashTarget,
};
use crate::i18n::{tr, tr_args, translate_widget_tree};
use crate::theme::{self, ThemeSnapshot};
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const DEFAULT_MINIMUM_SIZE: u64 = 100 * 1024 * 1024;
const MAX_TREEMAP_ITEMS: usize = 10;

#[derive(Clone)]
struct TreemapVisual {
    label: String,
    path: Option<PathBuf>,
    size: u64,
    color_index: usize,
}

#[derive(Clone)]
struct TreemapHit {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    path: Option<PathBuf>,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct StorageAnalyzerPage {
        pub analysis: RefCell<Option<StorageAnalysis>>,
        pub scan_root: RefCell<Option<PathBuf>>,
        pub current_path: RefCell<Option<PathBuf>>,
        pub selected: RefCell<BTreeMap<PathBuf, StorageNode>>,
        pub cancel_flag: RefCell<Option<Arc<AtomicBool>>>,
        pub operation_running: Cell<bool>,
        pub state_stack: RefCell<Option<gtk4::Stack>>,
        pub status_page: RefCell<Option<adw::StatusPage>>,
        pub choose_button: RefCell<Option<gtk4::Button>>,
        pub scan_button: RefCell<Option<gtk4::Button>>,
        pub threshold_combo: RefCell<Option<gtk4::ComboBoxText>>,
        pub root_label: RefCell<Option<gtk4::Label>>,
        pub item_count_label: RefCell<Option<gtk4::Label>>,
        pub total_size_label: RefCell<Option<gtk4::Label>>,
        pub warning_label: RefCell<Option<gtk4::Label>>,
        pub treemap: RefCell<Option<gtk4::DrawingArea>>,
        pub(super) treemap_hits: RefCell<Vec<TreemapHit>>,
        pub breadcrumb_box: RefCell<Option<gtk4::Box>>,
        pub results_list: RefCell<Option<gtk4::ListBox>>,
        pub selected_count_label: RefCell<Option<gtk4::Label>>,
        pub selected_size_label: RefCell<Option<gtk4::Label>>,
        pub trash_button: RefCell<Option<gtk4::Button>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StorageAnalyzerPage {
        const NAME: &'static str = "DataCleanerStorageAnalyzerPage";
        type Type = super::StorageAnalyzerPage;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for StorageAnalyzerPage {}
    impl WidgetImpl for StorageAnalyzerPage {}
    impl BoxImpl for StorageAnalyzerPage {}
}

glib::wrapper! {
    pub struct StorageAnalyzerPage(ObjectSubclass<imp::StorageAnalyzerPage>)
        @extends gtk4::Widget, gtk4::Box;
}

impl StorageAnalyzerPage {
    pub fn new() -> Self {
        let page: Self = glib::Object::builder()
            .property("orientation", gtk4::Orientation::Vertical)
            .property("spacing", 0)
            .build();
        page.setup_ui();
        page
    }

    fn setup_ui(&self) {
        let content = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(22)
            .margin_end(22)
            .vexpand(true)
            .hexpand(true)
            .build();
        content.add_css_class("storage-analyzer-page");
        self.append(&content);

        content.append(&self.create_toolbar());

        let state_stack = gtk4::Stack::new();
        state_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        state_stack.set_transition_duration(theme::transition_duration(self, 180));
        state_stack.set_vexpand(true);
        state_stack.set_hexpand(true);

        let status_page = adw::StatusPage::new();
        status_page.set_icon_name(Some("drive-harddisk-symbolic"));
        status_page.set_title("Analyze a Folder");
        status_page.set_description(Some(
            "Choose a local folder to find its largest files and subfolders.",
        ));
        state_stack.add_named(&status_page, Some("status"));

        let results = self.create_results_view();
        state_stack.add_named(&results, Some("results"));
        state_stack.set_visible_child_name("status");
        content.append(&state_stack);

        self.imp().state_stack.replace(Some(state_stack));
        self.imp().status_page.replace(Some(status_page));
    }

    fn create_toolbar(&self) -> gtk4::Widget {
        let toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        toolbar.add_css_class("storage-analyzer-toolbar");

        let choose_button = gtk4::Button::with_label("Choose Folder…");
        choose_button.set_icon_name("folder-open-symbolic");
        let page = self.downgrade();
        choose_button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                page.choose_folder();
            }
        });
        toolbar.append(&choose_button);

        let scan_button = gtk4::Button::with_label("Scan Again");
        scan_button.set_icon_name("view-refresh-symbolic");
        scan_button.add_css_class("suggested-action");
        scan_button.set_sensitive(false);
        let page = self.downgrade();
        scan_button.connect_clicked(move |button| {
            if let Some(page) = page.upgrade() {
                if page.operation_is_running() {
                    page.cancel_scan(button);
                } else if let Some(path) = page.imp().scan_root.borrow().clone() {
                    page.start_scan(path);
                }
            }
        });
        toolbar.append(&scan_button);

        let threshold_label = gtk4::Label::new(Some("Minimum size"));
        threshold_label.add_css_class("dim-label");
        threshold_label.set_margin_start(8);
        toolbar.append(&threshold_label);

        let threshold_combo = gtk4::ComboBoxText::new();
        threshold_combo.append(Some("104857600"), "100 MB");
        threshold_combo.append(Some("524288000"), "500 MB");
        threshold_combo.append(Some("1073741824"), "1 GB");
        threshold_combo.append(Some("5368709120"), "5 GB");
        threshold_combo.set_active_id(Some("104857600"));
        threshold_combo.set_tooltip_text(Some(&tr(
            "Files and folders smaller than this are grouped as Other",
        )));
        let page = self.downgrade();
        threshold_combo.connect_changed(move |_| {
            if let Some(page) = page.upgrade() {
                if !page.operation_is_running() && page.imp().analysis.borrow().is_some() {
                    if let Some(path) = page.imp().scan_root.borrow().clone() {
                        page.start_scan(path);
                    }
                }
            }
        });
        toolbar.append(&threshold_combo);

        let safety_label = gtk4::Label::new(Some("Manual analysis · same filesystem only"));
        safety_label.add_css_class("caption");
        safety_label.set_hexpand(true);
        safety_label.set_halign(gtk4::Align::End);
        toolbar.append(&safety_label);

        self.imp().choose_button.replace(Some(choose_button));
        self.imp().scan_button.replace(Some(scan_button));
        self.imp().threshold_combo.replace(Some(threshold_combo));
        toolbar.upcast()
    }

    fn create_results_view(&self) -> gtk4::Widget {
        let results = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        results.set_vexpand(true);

        let summary = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
        summary.add_css_class("storage-analyzer-summary");

        let root_label = gtk4::Label::new(None);
        root_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        root_label.set_hexpand(true);
        root_label.set_halign(gtk4::Align::Start);
        root_label.set_xalign(0.0);
        summary.append(&root_label);

        let item_count_label = gtk4::Label::new(None);
        item_count_label.add_css_class("dim-label");
        summary.append(&item_count_label);

        let total_size_label = gtk4::Label::new(None);
        total_size_label.add_css_class("heading");
        summary.append(&total_size_label);

        let warning_label = gtk4::Label::new(None);
        warning_label.add_css_class("warning");
        warning_label.set_tooltip_text(Some(&tr("Some entries could not be read")));
        summary.append(&warning_label);
        results.append(&summary);

        let treemap_frame = gtk4::Frame::new(None);
        treemap_frame.add_css_class("storage-treemap-frame");
        let treemap = gtk4::DrawingArea::new();
        treemap.set_size_request(-1, 160);
        treemap.set_hexpand(true);
        treemap.set_tooltip_text(Some(&tr("Click a folder block to inspect its contents")));
        let page_weak = self.downgrade();
        treemap.set_draw_func(move |area, cr, width, height| {
            if let Some(page) = page_weak.upgrade() {
                page.draw_treemap(area, cr, width, height);
            }
        });

        let click = gtk4::GestureClick::new();
        let page = self.downgrade();
        click.connect_released(move |_, _, x, y| {
            if let Some(page) = page.upgrade() {
                page.on_treemap_clicked(x, y);
            }
        });
        treemap.add_controller(click);
        treemap_frame.set_child(Some(&treemap));
        results.append(&treemap_frame);

        let breadcrumb_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        breadcrumb_box.add_css_class("storage-breadcrumbs");
        results.append(&breadcrumb_box);

        let header = Self::create_list_header();
        results.append(&header);

        let list_scrolled = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .min_content_height(140)
            .build();
        list_scrolled.add_css_class("storage-results-scroll");
        let results_list = gtk4::ListBox::new();
        results_list.set_selection_mode(gtk4::SelectionMode::None);
        results_list.add_css_class("storage-results-list");
        list_scrolled.set_child(Some(&results_list));
        results.append(&list_scrolled);

        let bottom_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        bottom_bar.add_css_class("storage-selection-bar");
        let selection_copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        selection_copy.set_hexpand(true);
        let selected_count_label = gtk4::Label::new(Some("Nothing selected"));
        selected_count_label.add_css_class("heading");
        selected_count_label.set_halign(gtk4::Align::Start);
        let selected_size_label = gtk4::Label::new(Some(
            "Select files or folders to review before moving them to Trash",
        ));
        selected_size_label.add_css_class("caption");
        selected_size_label.set_halign(gtk4::Align::Start);
        selection_copy.append(&selected_count_label);
        selection_copy.append(&selected_size_label);
        bottom_bar.append(&selection_copy);

        let trash_button = gtk4::Button::with_label("Move Selected to Trash");
        trash_button.set_icon_name("user-trash-symbolic");
        trash_button.add_css_class("destructive-action");
        trash_button.set_sensitive(false);
        let page = self.downgrade();
        trash_button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                page.confirm_move_to_trash();
            }
        });
        bottom_bar.append(&trash_button);
        results.append(&bottom_bar);

        self.imp().root_label.replace(Some(root_label));
        self.imp().item_count_label.replace(Some(item_count_label));
        self.imp().total_size_label.replace(Some(total_size_label));
        self.imp().warning_label.replace(Some(warning_label));
        self.imp().treemap.replace(Some(treemap));
        self.imp().breadcrumb_box.replace(Some(breadcrumb_box));
        self.imp().results_list.replace(Some(results_list));
        self.imp()
            .selected_count_label
            .replace(Some(selected_count_label));
        self.imp()
            .selected_size_label
            .replace(Some(selected_size_label));
        self.imp().trash_button.replace(Some(trash_button));
        results.upcast()
    }

    fn create_list_header() -> gtk4::Widget {
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        header.add_css_class("storage-results-header");

        let selection = gtk4::Label::new(None);
        selection.set_width_chars(2);
        header.append(&selection);

        let name = gtk4::Label::new(Some("Name"));
        name.set_hexpand(true);
        name.set_halign(gtk4::Align::Start);
        header.append(&name);

        let relative = gtk4::Label::new(Some("Relative size"));
        relative.set_width_chars(20);
        relative.set_halign(gtk4::Align::Start);
        header.append(&relative);

        let size = gtk4::Label::new(Some("Size"));
        size.set_width_chars(11);
        size.set_halign(gtk4::Align::End);
        header.append(&size);

        let open = gtk4::Label::new(None);
        open.set_width_chars(2);
        header.append(&open);
        header.upcast()
    }

    fn choose_folder(&self) {
        if self.operation_is_running() {
            return;
        }
        let window = self
            .root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok());
        let dialog = gtk4::FileChooserNative::new(
            Some(&tr("Choose a Folder to Analyze")),
            window.as_ref(),
            gtk4::FileChooserAction::SelectFolder,
            Some(&tr("Analyze")),
            Some(&tr("Cancel")),
        );
        if let Some(path) = self.imp().scan_root.borrow().as_ref() {
            let _ = dialog.set_current_folder(Some(&gio::File::for_path(path)));
        } else if let Some(home) = dirs::home_dir() {
            let _ = dialog.set_current_folder(Some(&gio::File::for_path(home)));
        }

        let page = self.downgrade();
        dialog.run_async(move |dialog, response| {
            if response == gtk4::ResponseType::Accept {
                if let Some(page) = page.upgrade() {
                    if let Some(path) = dialog.file().and_then(|file| file.path()) {
                        page.start_scan(path);
                    } else {
                        page.show_info_dialog(
                            &tr("Local Folders Only"),
                            &tr("Storage Analyzer currently supports local folders only."),
                        );
                    }
                }
            }
            dialog.destroy();
        });
    }

    fn minimum_size(&self) -> u64 {
        self.imp()
            .threshold_combo
            .borrow()
            .as_ref()
            .and_then(|combo| combo.active_id())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_MINIMUM_SIZE)
    }

    fn start_scan(&self, path: PathBuf) {
        if self.operation_is_running() {
            return;
        }
        if self.dashboard_operation_is_running() {
            self.show_info_dialog(
                &tr("Operation in Progress"),
                &tr("Wait for the current cleanup operation to finish before analyzing storage."),
            );
            return;
        }

        let cancellation = Arc::new(AtomicBool::new(false));
        self.imp().cancel_flag.replace(Some(cancellation.clone()));
        self.imp().operation_running.set(true);
        self.set_controls_for_operation(true, &tr("Cancel Scan"));
        self.show_scanning_status(&path);

        let options = StorageScanOptions {
            minimum_size: self.minimum_size(),
            ..Default::default()
        };
        let (sender, receiver) = async_channel::bounded(1);
        crate::runtime().spawn_blocking(move || {
            let result = analyze_storage(&path, options, cancellation);
            let _ = sender.send_blocking(result);
        });

        let page = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(result) = receiver.recv().await {
                page.finish_scan(result);
            } else {
                page.finish_scan(Err(StorageAnalysisError::Cancelled));
            }
        });
    }

    fn cancel_scan(&self, button: &gtk4::Button) {
        if let Some(flag) = self.imp().cancel_flag.borrow().as_ref() {
            flag.store(true, Ordering::Relaxed);
            button.set_label(&tr("Cancelling…"));
            button.set_sensitive(false);
        }
    }

    fn finish_scan(&self, result: Result<StorageAnalysis, StorageAnalysisError>) {
        self.imp().operation_running.set(false);
        self.imp().cancel_flag.replace(None);
        self.set_controls_for_operation(false, "Scan Again");

        match result {
            Ok(analysis) => {
                let root = analysis.root.path.clone();
                self.imp().scan_root.replace(Some(root.clone()));
                self.imp().current_path.replace(Some(root));
                self.imp().analysis.replace(Some(analysis));
                self.imp().selected.borrow_mut().clear();
                if let Some(stack) = self.imp().state_stack.borrow().as_ref() {
                    stack.set_visible_child_name("results");
                }
                self.refresh_results();
            }
            Err(StorageAnalysisError::Cancelled) => {
                if self.imp().analysis.borrow().is_some() {
                    if let Some(stack) = self.imp().state_stack.borrow().as_ref() {
                        stack.set_visible_child_name("results");
                    }
                } else {
                    self.show_status(
                        "drive-harddisk-symbolic",
                        &tr("Scan Cancelled"),
                        &tr("Choose a folder when you are ready to analyze storage."),
                    );
                }
            }
            Err(error) => self.show_status(
                "dialog-error-symbolic",
                &tr("Could Not Analyze Folder"),
                &error.to_string(),
            ),
        }
    }

    fn set_controls_for_operation(&self, running: bool, scan_label: &str) {
        if let Some(button) = self.imp().choose_button.borrow().as_ref() {
            button.set_sensitive(!running);
        }
        if let Some(button) = self.imp().scan_button.borrow().as_ref() {
            button.set_label(&tr(scan_label));
            button.set_icon_name(if running {
                "process-stop-symbolic"
            } else {
                "view-refresh-symbolic"
            });
            button.set_sensitive(running || self.imp().scan_root.borrow().is_some());
        }
        if let Some(combo) = self.imp().threshold_combo.borrow().as_ref() {
            combo.set_sensitive(!running);
        }
        if let Some(button) = self.imp().trash_button.borrow().as_ref() {
            button.set_sensitive(!running && !self.imp().selected.borrow().is_empty());
        }
    }

    fn show_scanning_status(&self, path: &Path) {
        let spinner = gtk4::Spinner::new();
        spinner.set_size_request(32, 32);
        spinner.start();
        if let Some(status) = self.imp().status_page.borrow().as_ref() {
            status.set_icon_name(None);
            status.set_title(&tr("Analyzing Storage…"));
            status.set_description(Some(&tr_args(
                "Scanning {path} without following symbolic links",
                &[("{path}", &path.display().to_string())],
            )));
            status.set_child(Some(&spinner));
        }
        if let Some(stack) = self.imp().state_stack.borrow().as_ref() {
            stack.set_visible_child_name("status");
        }
    }

    fn show_status(&self, icon: &str, title: &str, description: &str) {
        if let Some(status) = self.imp().status_page.borrow().as_ref() {
            status.set_child(Option::<&gtk4::Widget>::None);
            status.set_icon_name(Some(icon));
            status.set_title(&tr(title));
            status.set_description(Some(&tr(description)));
        }
        if let Some(stack) = self.imp().state_stack.borrow().as_ref() {
            stack.set_visible_child_name("status");
        }
    }

    fn current_node(&self) -> Option<StorageNode> {
        let current = self.imp().current_path.borrow().clone()?;
        self.imp()
            .analysis
            .borrow()
            .as_ref()
            .and_then(|analysis| analysis.root.find(&current).cloned())
    }

    fn refresh_results(&self) {
        let Some(analysis) = self.imp().analysis.borrow().clone() else {
            return;
        };
        if let Some(label) = self.imp().root_label.borrow().as_ref() {
            label.set_text(&analysis.root.path.display().to_string());
            label.set_tooltip_text(Some(&analysis.root.path.display().to_string()));
        }
        if let Some(label) = self.imp().item_count_label.borrow().as_ref() {
            label.set_text(&tr_args(
                "{count} items",
                &[("{count}", &analysis.item_count().to_string())],
            ));
        }
        if let Some(label) = self.imp().total_size_label.borrow().as_ref() {
            label.set_text(&bytesize::ByteSize(analysis.root.size).to_string());
        }
        if let Some(label) = self.imp().warning_label.borrow().as_ref() {
            let warning = if analysis.skipped.is_empty() {
                String::new()
            } else {
                tr("Some entries skipped")
            };
            label.set_text(&warning);
            label.set_tooltip_text(Some(&if analysis.skipped.is_empty() {
                tr("All readable entries were scanned")
            } else {
                tr_args(
                    "{count} entries could not be read",
                    &[("{count}", &analysis.skipped.len().to_string())],
                )
            }));
        }

        self.refresh_breadcrumbs();
        self.refresh_result_rows();
        self.refresh_selection_summary();
        if let Some(treemap) = self.imp().treemap.borrow().as_ref() {
            treemap.queue_draw();
        }
    }

    fn refresh_breadcrumbs(&self) {
        let Some(container) = self.imp().breadcrumb_box.borrow().as_ref().cloned() else {
            return;
        };
        while let Some(child) = container.first_child() {
            container.remove(&child);
        }
        let Some(analysis) = self.imp().analysis.borrow().as_ref().cloned() else {
            return;
        };
        let Some(current) = self.imp().current_path.borrow().clone() else {
            return;
        };

        let mut paths = vec![analysis.root.path.clone()];
        if let Ok(relative) = current.strip_prefix(&analysis.root.path) {
            let mut path = analysis.root.path.clone();
            for component in relative.components() {
                path.push(component.as_os_str());
                paths.push(path.clone());
            }
        }

        for (index, path) in paths.into_iter().enumerate() {
            if index > 0 {
                let separator = gtk4::Label::new(Some("›"));
                separator.add_css_class("dim-label");
                container.append(&separator);
            }
            let name = if index == 0 {
                analysis.root.display_name()
            } else {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            };
            let button = gtk4::Button::with_label(&name);
            button.add_css_class("flat");
            button.add_css_class("storage-breadcrumb");
            let page = self.downgrade();
            button.connect_clicked(move |_| {
                if let Some(page) = page.upgrade() {
                    page.navigate_to(&path);
                }
            });
            container.append(&button);
        }
    }

    fn refresh_result_rows(&self) {
        let Some(list) = self.imp().results_list.borrow().as_ref().cloned() else {
            return;
        };
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        let Some(current) = self.current_node() else {
            return;
        };

        if current.children.is_empty() {
            let row = gtk4::ListBoxRow::new();
            row.set_selectable(false);
            let label = gtk4::Label::new(Some(&tr(
                "No individual files or folders meet the selected size threshold",
            )));
            label.add_css_class("dim-label");
            label.set_margin_top(24);
            label.set_margin_bottom(24);
            label.set_wrap(true);
            row.set_child(Some(&label));
            list.append(&row);
            return;
        }

        for node in current.children.clone() {
            list.append(&self.create_result_row(node, current.size));
        }
    }

    fn create_result_row(&self, node: StorageNode, parent_size: u64) -> gtk4::ListBoxRow {
        let row = gtk4::ListBoxRow::new();
        row.set_selectable(false);
        row.set_activatable(false);
        row.add_css_class("storage-result-row");

        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        content.set_margin_top(7);
        content.set_margin_bottom(7);
        content.set_margin_start(8);
        content.set_margin_end(8);

        let check = gtk4::CheckButton::new();
        check.set_tooltip_text(Some(&if node.is_directory() {
            tr("Select this entire folder and all of its contents")
        } else {
            tr("Select this file")
        }));
        check.set_active(self.imp().selected.borrow().contains_key(&node.path));
        let page = self.downgrade();
        let selected_node = node.clone();
        check.connect_toggled(move |check| {
            if let Some(page) = page.upgrade() {
                let accepted = page.update_node_selection(&selected_node, check.is_active());
                if check.is_active() && !accepted {
                    check.set_active(false);
                }
            }
        });
        content.append(&check);

        let identity = gtk4::Box::new(gtk4::Orientation::Horizontal, 9);
        identity.set_hexpand(true);
        let icon = gtk4::Image::from_icon_name(if node.is_directory() {
            "folder-symbolic"
        } else {
            "text-x-generic-symbolic"
        });
        icon.add_css_class("accent");
        identity.append(&icon);
        let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        labels.set_hexpand(true);
        let name = gtk4::Label::new(Some(&node.display_name()));
        name.set_halign(gtk4::Align::Start);
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        name.add_css_class("heading");
        let details = gtk4::Label::new(Some(&Self::node_details(&node)));
        details.set_halign(gtk4::Align::Start);
        details.set_xalign(0.0);
        details.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        details.add_css_class("caption");
        details.set_tooltip_text(Some(&node.path.display().to_string()));
        labels.append(&name);
        labels.append(&details);
        identity.append(&labels);
        content.append(&identity);

        let progress = gtk4::ProgressBar::new();
        progress.set_width_request(150);
        progress.set_fraction(if parent_size > 0 {
            node.size as f64 / parent_size as f64
        } else {
            0.0
        });
        progress.set_valign(gtk4::Align::Center);
        progress.add_css_class("storage-size-bar");
        content.append(&progress);

        let size = gtk4::Label::new(Some(&bytesize::ByteSize(node.size).to_string()));
        size.set_width_chars(11);
        size.set_halign(gtk4::Align::End);
        size.add_css_class("heading");
        content.append(&size);

        if node.is_directory() {
            let open = gtk4::Button::from_icon_name("go-next-symbolic");
            open.add_css_class("flat");
            open.set_tooltip_text(Some(&tr("Inspect this folder")));
            let page = self.downgrade();
            let path = node.path.clone();
            open.connect_clicked(move |_| {
                if let Some(page) = page.upgrade() {
                    page.navigate_to(&path);
                }
            });
            content.append(&open);
        } else {
            let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            spacer.set_width_request(34);
            content.append(&spacer);
        }

        row.set_child(Some(&content));
        row
    }

    fn node_details(node: &StorageNode) -> String {
        let kind = if node.is_directory() {
            tr("Folder")
        } else {
            tr("File")
        };
        let modified = node
            .modified
            .map(chrono::DateTime::<chrono::Local>::from)
            .map(|date| date.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| tr("date unavailable"));
        tr_args(
            "{kind} · Modified {date} · {path}",
            &[
                ("{kind}", &kind),
                ("{date}", &modified),
                ("{path}", &node.path.display().to_string()),
            ],
        )
    }

    fn update_node_selection(&self, node: &StorageNode, active: bool) -> bool {
        let mut selected = self.imp().selected.borrow_mut();
        if !active {
            selected.remove(&node.path);
            drop(selected);
            self.refresh_selection_summary();
            return true;
        }

        let covered = selected.values().any(|parent| {
            parent.is_directory() && parent.path != node.path && node.path.starts_with(&parent.path)
        });
        if covered {
            drop(selected);
            self.refresh_selection_summary();
            return false;
        }

        if node.is_directory() {
            selected.retain(|path, _| path == &node.path || !path.starts_with(&node.path));
        }
        selected.insert(node.path.clone(), node.clone());
        drop(selected);
        self.refresh_selection_summary();
        true
    }

    fn refresh_selection_summary(&self) {
        let selected = self.imp().selected.borrow();
        let count = selected.len();
        let total: u64 = selected.values().map(|node| node.size).sum();
        if let Some(label) = self.imp().selected_count_label.borrow().as_ref() {
            let text = if count == 0 {
                tr("Nothing selected")
            } else if count == 1 {
                tr("1 item selected")
            } else {
                tr_args(
                    "{count} items selected",
                    &[("{count}", &count.to_string())],
                )
            };
            label.set_text(&text);
        }
        if let Some(label) = self.imp().selected_size_label.borrow().as_ref() {
            let text = if count == 0 {
                tr(
                    "Select files or folders to review before moving them to Trash",
                )
            } else {
                tr_args(
                    "{size} will be reviewed before moving",
                    &[("{size}", &bytesize::ByteSize(total).to_string())],
                )
            };
            label.set_text(&text);
        }
        if let Some(button) = self.imp().trash_button.borrow().as_ref() {
            button.set_sensitive(count > 0 && !self.operation_is_running());
        }
    }

    fn navigate_to(&self, path: &Path) {
        let exists = self
            .imp()
            .analysis
            .borrow()
            .as_ref()
            .and_then(|analysis| analysis.root.find(path))
            .is_some();
        if !exists {
            return;
        }
        self.imp().current_path.replace(Some(path.to_path_buf()));
        self.refresh_breadcrumbs();
        self.refresh_result_rows();
        if let Some(treemap) = self.imp().treemap.borrow().as_ref() {
            treemap.queue_draw();
        }
    }

    fn treemap_visuals(&self) -> Vec<TreemapVisual> {
        let Some(current) = self.current_node() else {
            return Vec::new();
        };
        let keep = current
            .children
            .len()
            .min(MAX_TREEMAP_ITEMS.saturating_sub(1));
        let mut visuals: Vec<_> = current
            .children
            .iter()
            .take(keep)
            .enumerate()
            .map(|(index, node)| TreemapVisual {
                label: node.display_name(),
                path: if node.is_directory() {
                    Some(node.path.clone())
                } else {
                    None
                },
                size: node.size,
                color_index: index,
            })
            .collect();
        let shown: u64 = visuals.iter().map(|item| item.size).sum();
        let other = current.size.saturating_sub(shown);
        if other > 0 {
            visuals.push(TreemapVisual {
                label: tr("Other"),
                path: None,
                size: other,
                color_index: usize::MAX,
            });
        }
        visuals
    }

    fn draw_treemap(
        &self,
        area: &gtk4::DrawingArea,
        cr: &gtk4::cairo::Context,
        width: i32,
        height: i32,
    ) {
        let theme = ThemeSnapshot::from_widget(area);
        let visuals = self.treemap_visuals();
        let mut layouts = Vec::new();
        layout_binary(
            &visuals,
            5.0,
            5.0,
            (width as f64 - 10.0).max(0.0),
            (height as f64 - 10.0).max(0.0),
            &mut layouts,
        );

        let (bg_r, bg_g, bg_b, bg_a) = theme::rgba_to_cairo(&theme.shade);
        cr.set_source_rgba(bg_r, bg_g, bg_b, bg_a.max(0.16));
        cr.rectangle(0.0, 0.0, width as f64, height as f64);
        let _ = cr.fill();

        let mut hits = Vec::new();
        for (visual, x, y, rect_width, rect_height) in layouts {
            let padding = 2.5;
            let x = x + padding;
            let y = y + padding;
            let rect_width = (rect_width - padding * 2.0).max(0.0);
            let rect_height = (rect_height - padding * 2.0).max(0.0);
            if rect_width < 2.0 || rect_height < 2.0 {
                continue;
            }
            let (red, green, blue) = treemap_color(visual.color_index, theme.is_dark);
            rounded_rectangle(cr, x, y, rect_width, rect_height, 6.0);
            cr.set_source_rgb(red, green, blue);
            let _ = cr.fill();

            hits.push(TreemapHit {
                x,
                y,
                width: rect_width,
                height: rect_height,
                path: visual.path.clone(),
            });

            if rect_width >= 72.0 && rect_height >= 38.0 {
                let _ = cr.save();
                rounded_rectangle(cr, x, y, rect_width, rect_height, 6.0);
                cr.clip();
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.96);
                cr.select_font_face(
                    "Sans",
                    gtk4::cairo::FontSlant::Normal,
                    gtk4::cairo::FontWeight::Bold,
                );
                cr.set_font_size(12.0);
                cr.move_to(x + 9.0, y + 18.0);
                let _ = cr.show_text(&truncate_label(&visual.label, rect_width));
                if rect_height >= 56.0 {
                    cr.select_font_face(
                        "Sans",
                        gtk4::cairo::FontSlant::Normal,
                        gtk4::cairo::FontWeight::Normal,
                    );
                    cr.set_font_size(10.0);
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.78);
                    cr.move_to(x + 9.0, y + 35.0);
                    let _ = cr.show_text(&bytesize::ByteSize(visual.size).to_string());
                }
                let _ = cr.restore();
            }
        }
        self.imp().treemap_hits.replace(hits);
    }

    fn on_treemap_clicked(&self, x: f64, y: f64) {
        let path = self
            .imp()
            .treemap_hits
            .borrow()
            .iter()
            .find(|hit| {
                x >= hit.x && x <= hit.x + hit.width && y >= hit.y && y <= hit.y + hit.height
            })
            .and_then(|hit| hit.path.clone());
        if let Some(path) = path {
            self.navigate_to(&path);
        }
    }

    fn confirm_move_to_trash(&self) {
        if self.operation_is_running() {
            return;
        }
        let selected: Vec<_> = self.imp().selected.borrow().values().cloned().collect();
        if selected.is_empty() {
            return;
        }
        let total: u64 = selected.iter().map(|node| node.size).sum();
        let preview = selected
            .iter()
            .take(5)
            .map(|node| format!("• {}", node.path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        let remainder = selected.len().saturating_sub(5);
        let more = if remainder > 0 {
            tr_args(
                "\n• and {count} more",
                &[("{count}", &remainder.to_string())],
            )
        } else {
            String::new()
        };
        let message = tr_args(
            "Move {count} selected {item_word} ({size}) to Trash?\n\n{preview}{more}\n\nFolders include all contents, including smaller items grouped as Other. Items can be restored from Trash.",
            &[
                ("{count}", &selected.len().to_string()),
                (
                    "{item_word}",
                    &if selected.len() == 1 {
                        tr("item")
                    } else {
                        tr("items")
                    },
                ),
                ("{size}", &bytesize::ByteSize(total).to_string()),
                ("{preview}", &preview),
                ("{more}", &more),
            ],
        );
        let window = self
            .root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok());
        let dialog = adw::MessageDialog::new(
            window.as_ref(),
            Some(&tr("Move Selected Items to Trash?")),
            Some(&message),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("trash", "Move to Trash");
        dialog.set_response_appearance("trash", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let page = self.downgrade();
        dialog.connect_response(None, move |_, response| {
            if response == "trash" {
                if let Some(page) = page.upgrade() {
                    page.start_trash_operation(selected.clone());
                }
            }
        });
        translate_widget_tree(&dialog);
        dialog.present();
    }

    fn start_trash_operation(&self, selected: Vec<StorageNode>) {
        let Some(scan_root) = self.imp().scan_root.borrow().clone() else {
            return;
        };
        if self.dashboard_operation_is_running() {
            self.show_info_dialog(
                &tr("Operation in Progress"),
                &tr("Wait for the current cleanup operation before moving storage items."),
            );
            return;
        }
        self.imp().operation_running.set(true);
        self.set_controls_for_operation(true, &tr("Moving…"));
        if let Some(button) = self.imp().scan_button.borrow().as_ref() {
            button.set_sensitive(false);
        }
        let targets: Vec<TrashTarget> = selected.iter().map(TrashTarget::from).collect();
        let (sender, receiver) = async_channel::bounded(1);
        let worker_root = scan_root.clone();
        crate::runtime().spawn_blocking(move || {
            let result = move_to_trash(&worker_root, &targets);
            let _ = sender.send_blocking(result);
        });

        let page = self.clone();
        glib::spawn_future_local(async move {
            let Ok(result) = receiver.recv().await else {
                page.imp().operation_running.set(false);
                page.set_controls_for_operation(false, "Scan Again");
                page.show_info_dialog(
                    &tr("Trash Operation Failed"),
                    &tr("The operation did not return a result."),
                );
                return;
            };
            page.imp().operation_running.set(false);
            page.set_controls_for_operation(false, "Scan Again");
            for (path, _) in &result.moved {
                page.imp().selected.borrow_mut().remove(path);
            }
            page.refresh_selection_summary();

            let moved = result.moved.len();
            let failed = result.failed.len();
            let failure_details = result
                .failed
                .iter()
                .take(3)
                .map(|(path, reason)| format!("\n{}: {}", path.display(), reason))
                .collect::<String>();
            let message = tr_args(
                "Moved {moved} {item_word} ({size}) to Trash.\n{failed} items failed.{details}",
                &[
                    ("{moved}", &moved.to_string()),
                    (
                        "{item_word}",
                        &if moved == 1 { tr("item") } else { tr("items") },
                    ),
                    ("{size}", &bytesize::ByteSize(result.bytes_moved()).to_string()),
                    ("{failed}", &failed.to_string()),
                    ("{details}", &failure_details),
                ],
            );
            page.show_trash_result(&message, moved > 0, scan_root);
        });
    }

    fn show_trash_result(&self, message: &str, rescan: bool, scan_root: PathBuf) {
        let window = self
            .root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok());
        let dialog = adw::MessageDialog::new(
            window.as_ref(),
            Some(&if rescan {
                tr("Items Moved to Trash")
            } else {
                tr("Nothing Was Moved")
            }),
            Some(message),
        );
        dialog.add_response("ok", "OK");
        dialog.set_default_response(Some("ok"));
        let page = self.downgrade();
        dialog.connect_response(None, move |_, _| {
            if rescan {
                if let Some(page) = page.upgrade() {
                    page.start_scan(scan_root.clone());
                }
            }
        });
        translate_widget_tree(&dialog);
        dialog.present();
    }

    fn dashboard_operation_is_running(&self) -> bool {
        self.root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok())
            .and_then(|window| window.downcast::<crate::ui::MainWindow>().ok())
            .map(|window| {
                window.dashboard_operation_is_running() || window.system_operation_is_running()
            })
            .unwrap_or(false)
    }

    pub fn operation_is_running(&self) -> bool {
        self.imp().operation_running.get()
    }

    pub fn show_operation_running_dialog(&self) {
        self.show_info_dialog(
            &tr("Storage Operation in Progress"),
            &tr("Cancel the current storage scan or wait for the Trash operation to finish before closing Data Cleaner."),
        );
    }

    fn show_info_dialog(&self, title: &str, message: &str) {
        let window = self
            .root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok());
        let dialog = adw::MessageDialog::new(
            window.as_ref(),
            Some(&tr(title)),
            Some(&tr(message)),
        );
        dialog.add_response("ok", "OK");
        dialog.set_default_response(Some("ok"));
        translate_widget_tree(&dialog);
        dialog.present();
    }
}

impl Default for StorageAnalyzerPage {
    fn default() -> Self {
        Self::new()
    }
}

fn layout_binary<'a>(
    items: &'a [TreemapVisual],
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    output: &mut Vec<(&'a TreemapVisual, f64, f64, f64, f64)>,
) {
    if items.is_empty() || width <= 0.0 || height <= 0.0 {
        return;
    }
    if items.len() == 1 {
        output.push((&items[0], x, y, width, height));
        return;
    }

    let total: u64 = items.iter().map(|item| item.size).sum();
    if total == 0 {
        return;
    }
    let half = total as f64 / 2.0;
    let mut running = 0_u64;
    let mut split = 1usize;
    let mut best_distance = f64::MAX;
    for index in 1..items.len() {
        running = running.saturating_add(items[index - 1].size);
        let distance = (running as f64 - half).abs();
        if distance < best_distance {
            best_distance = distance;
            split = index;
        }
    }
    let first_total: u64 = items[..split].iter().map(|item| item.size).sum();
    let ratio = first_total as f64 / total as f64;
    if width >= height {
        let first_width = width * ratio;
        layout_binary(&items[..split], x, y, first_width, height, output);
        layout_binary(
            &items[split..],
            x + first_width,
            y,
            width - first_width,
            height,
            output,
        );
    } else {
        let first_height = height * ratio;
        layout_binary(&items[..split], x, y, width, first_height, output);
        layout_binary(
            &items[split..],
            x,
            y + first_height,
            width,
            height - first_height,
            output,
        );
    }
}

fn treemap_color(index: usize, dark: bool) -> (f64, f64, f64) {
    if index == usize::MAX {
        return if dark {
            (0.35, 0.36, 0.39)
        } else {
            (0.48, 0.49, 0.52)
        };
    }
    const COLORS: &[(u8, u8, u8)] = &[
        (49, 95, 146),
        (106, 79, 151),
        (58, 123, 113),
        (153, 87, 92),
        (139, 107, 55),
        (56, 113, 155),
        (113, 90, 132),
        (73, 126, 80),
        (157, 94, 61),
    ];
    let (red, green, blue) = COLORS[index % COLORS.len()];
    (
        red as f64 / 255.0,
        green as f64 / 255.0,
        blue as f64 / 255.0,
    )
}

fn rounded_rectangle(
    cr: &gtk4::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    cr.new_sub_path();
    cr.arc(
        x + width - radius,
        y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    cr.arc(
        x + radius,
        y + height - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        x + radius,
        y + radius,
        radius,
        std::f64::consts::PI,
        std::f64::consts::PI * 1.5,
    );
    cr.close_path();
}

fn truncate_label(label: &str, width: f64) -> String {
    let maximum = ((width - 18.0) / 7.0).floor().max(3.0) as usize;
    let count = label.chars().count();
    if count <= maximum {
        label.to_string()
    } else {
        let take = maximum.saturating_sub(1);
        format!("{}…", label.chars().take(take).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_layout_preserves_total_area() {
        let items = vec![
            TreemapVisual {
                label: "A".to_string(),
                path: None,
                size: 60,
                color_index: 0,
            },
            TreemapVisual {
                label: "B".to_string(),
                path: None,
                size: 30,
                color_index: 1,
            },
            TreemapVisual {
                label: "C".to_string(),
                path: None,
                size: 10,
                color_index: 2,
            },
        ];
        let mut layout = Vec::new();
        layout_binary(&items, 0.0, 0.0, 100.0, 50.0, &mut layout);
        let area: f64 = layout
            .iter()
            .map(|(_, _, _, width, height)| width * height)
            .sum();
        assert!((area - 5000.0).abs() < 0.001);
    }

    #[test]
    fn labels_are_truncated_on_character_boundaries() {
        assert_eq!(truncate_label("αβγδεζηθ", 40.0), "αβ…");
    }
}
