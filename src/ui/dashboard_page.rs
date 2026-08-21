use crate::models::ScanResult;
use crate::services::{vacuum_system_journal, CleanOptions, Cleaner, ScanOptions, Scanner};
use crate::storage::Storage;
use crate::theme::{self, ThemeSnapshot};
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use sysinfo::Disks;

struct DiskUsage {
    total: u64,
    available: u64,
    device_name: String,
    mount_point: PathBuf,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DashboardPage {
        pub storage: RefCell<Option<Arc<Storage>>>,
        pub space_label: RefCell<Option<gtk4::Label>>,
        pub disk_summary_label: RefCell<Option<gtk4::Label>>,
        pub disk_context_label: RefCell<Option<gtk4::Label>>,
        pub disk_available_label: RefCell<Option<gtk4::Label>>,
        pub disk_percent_label: RefCell<Option<gtk4::Label>>,
        pub targets_label: RefCell<Option<gtk4::Label>>,
        pub last_clean_label: RefCell<Option<gtk4::Label>>,
        pub clean_button: RefCell<Option<gtk4::Button>>,
        pub scan_button: RefCell<Option<gtk4::Button>>,
        pub cancel_button: RefCell<Option<gtk4::Button>>,
        pub view_log_button: RefCell<Option<gtk4::Button>>,
        pub scan_result: RefCell<Option<ScanResult>>,
        pub donut_area: RefCell<Option<gtk4::DrawingArea>>,
        pub reclaimable_bytes: RefCell<u64>,
        pub total_disk_bytes: RefCell<u64>,
        pub available_disk_bytes: RefCell<u64>,
        pub has_scanned: RefCell<bool>,
        pub operation_running: RefCell<Option<Arc<AtomicBool>>>,
        pub operation_cancel: RefCell<Option<Arc<AtomicBool>>>,
        pub disk_refresh_running: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DashboardPage {
        const NAME: &'static str = "DataCleanerDashboardPage";
        type Type = super::DashboardPage;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for DashboardPage {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for DashboardPage {}
    impl BoxImpl for DashboardPage {}
}

glib::wrapper! {
    pub struct DashboardPage(ObjectSubclass<imp::DashboardPage>)
        @extends gtk4::Widget, gtk4::Box;
}

impl DashboardPage {
    pub fn new(storage: Arc<Storage>) -> Self {
        let page: Self = glib::Object::builder()
            .property("orientation", gtk4::Orientation::Vertical)
            .property("spacing", 0)
            .build();

        page.imp().storage.replace(Some(storage));
        page.imp().operation_running.replace(Some(Arc::new(AtomicBool::new(false))));
        page.setup_ui();
        page.refresh();
        page
    }

    fn setup_ui(&self) {
        self.set_margin_top(0);
        self.set_margin_bottom(24);
        self.set_margin_start(24);
        self.set_margin_end(24);

        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);

        // Keep the welcome and storage sections visually connected. The
        // action section below retains the previous 24px separation.
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        // Keep the welcome section flush with the top of the Dashboard while
        // preserving the spacing between the sections below it.
        content.set_margin_top(0);

        // === WELCOME HEADER CARD ===
        let header_card = self.create_welcome_header();
        content.append(&header_card);

        // === STORAGE OVERVIEW ===
        let storage_overview = self.create_storage_overview();
        content.append(&storage_overview);

        // === ACTION SECTION ===
        let action_section = self.create_action_section();
        action_section.set_margin_top(16);
        content.append(&action_section);

        scrolled.set_child(Some(&content));
        self.append(&scrolled);
    }

    fn create_welcome_header(&self) -> gtk4::Widget {
        let container = gtk4::Frame::new(None);
        container.add_css_class("dashboard-section");
        container.add_css_class("welcome-header");
        container.set_overflow(gtk4::Overflow::Hidden);

        let overlay = gtk4::Overlay::new();

        // Background decoration with cleaning graphic (right side)
        let decoration = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        decoration.set_halign(gtk4::Align::End);
        decoration.set_valign(gtk4::Align::Fill);

        // Create the cleaning graphic using DrawingArea
        let drawing = gtk4::DrawingArea::new();
        drawing.set_size_request(200, 150);
        drawing.set_halign(gtk4::Align::End);
        drawing.set_valign(gtk4::Align::Center);
        drawing.add_css_class("welcome-graphic");

        drawing.set_draw_func(|area, cr, width, height| {
            let w = width as f64;
            let h = height as f64;
            let theme = ThemeSnapshot::from_widget(area);
            // Match the STT waveform approach: the DrawingArea receives the
            // live GNOME accent through CSS and Cairo uses its computed color.
            let accent = area.style_context().color();
            let (accent_r, accent_g, accent_b, accent_a) = theme::rgba_to_cairo(&accent);

            // Render the eraser 30% smaller than its previous 70%-of-height size
            // and keep the 480x480 viewBox centered in the same drawing area.
            let scale = (h * 0.49) / 480.0;
            let offset_x = w * 0.5 - (240.0 * scale);
            let offset_y = (h - 480.0 * scale) * 0.5;

            cr.save().unwrap();
            cr.translate(offset_x, offset_y);
            cr.scale(scale, scale);

            // Draw the eraser path with the current standalone accent.
            cr.set_source_rgba(
                accent_r,
                accent_g,
                accent_b,
                accent_a * if theme.is_high_contrast { 0.88 } else { 0.72 },
            );

            // Main eraser path
            cr.move_to(333.142, 350.846);
            cr.curve_to(333.257, 350.731, 333.357, 350.607, 333.465, 350.489);
            cr.line_to(463.146, 220.783);
            cr.curve_to(474.024, 209.905, 480.01, 195.415, 480.001, 179.983);
            cr.curve_to(479.991, 164.574, 474.002, 150.118, 463.147, 139.289);
            cr.line_to(365.303, 41.415);
            cr.curve_to(354.45, 30.57, 339.977, 24.598, 324.553, 24.598);
            cr.curve_to(309.127, 24.598, 294.658, 30.572, 283.812, 41.418);
            cr.line_to(16.855, 308.329);
            cr.curve_to(5.974, 319.21, -0.012, 333.713, 0.0, 349.168);
            cr.curve_to(0.013, 364.593, 6.002, 379.052, 16.854, 389.868);
            cr.line_to(79.446, 452.474);
            cr.curve_to(79.507, 452.535, 79.573, 452.586, 79.634, 452.645);
            cr.curve_to(79.808, 452.81, 79.983, 452.976, 80.168, 453.128);
            cr.curve_to(80.25, 453.195, 80.339, 453.254, 80.423, 453.318);
            cr.curve_to(80.598, 453.453, 80.772, 453.589, 80.955, 453.713);
            cr.curve_to(81.025, 453.76, 81.1, 453.798, 81.17, 453.843);
            cr.curve_to(81.375, 453.974, 81.582, 454.103, 81.797, 454.219);
            cr.curve_to(81.848, 454.245, 81.9, 454.267, 81.951, 454.293);
            cr.curve_to(82.19, 454.416, 82.433, 454.534, 82.683, 454.639);
            cr.curve_to(82.716, 454.653, 82.75, 454.663, 82.784, 454.676);
            cr.curve_to(83.053, 454.784, 83.324, 454.884, 83.603, 454.969);
            cr.curve_to(83.637, 454.98, 83.673, 454.986, 83.707, 454.996);
            cr.curve_to(83.983, 455.077, 84.263, 455.15, 84.548, 455.207);
            cr.curve_to(84.63, 455.224, 84.713, 455.23, 84.795, 455.245);
            cr.curve_to(85.034, 455.286, 85.274, 455.329, 85.519, 455.352);
            cr.curve_to(85.849, 455.385, 86.182, 455.403, 86.517, 455.403);
            cr.line_to(224.427, 455.403);
            cr.line_to(383.735, 455.403);
            cr.curve_to(389.257, 455.403, 393.735, 450.925, 393.735, 445.403);
            cr.curve_to(393.735, 439.881, 389.257, 435.403, 383.735, 435.403);
            cr.line_to(248.566, 435.403);
            cr.line_to(332.786, 351.167);
            cr.curve_to(332.904, 351.06, 333.027, 350.96, 333.142, 350.846);
            cr.close_path();

            cr.move_to(220.285, 435.404);
            cr.line_to(90.66, 435.404);
            cr.line_to(30.985, 375.715);
            cr.curve_to(23.909, 368.661, 20.008, 359.228, 20.0, 349.152);
            cr.curve_to(19.992, 339.046, 23.897, 329.57, 30.996, 322.471);
            cr.line_to(160.821, 192.668);
            cr.line_to(311.912, 343.759);
            cr.line_to(220.285, 435.404);
            cr.close_path();

            cr.move_to(174.965, 178.527);
            cr.line_to(297.953, 55.56);
            cr.curve_to(305.022, 48.491, 314.469, 44.597, 324.553, 44.597);
            cr.curve_to(334.638, 44.597, 344.089, 48.492, 351.162, 55.559);
            cr.line_to(449.012, 153.439);
            cr.curve_to(456.092, 160.502, 459.994, 169.932, 460.001, 179.996);
            cr.curve_to(460.007, 190.081, 456.102, 199.543, 449.003, 206.641);
            cr.line_to(326.053, 329.615);
            cr.line_to(174.965, 178.527);
            cr.close_path();

            let _ = cr.fill();

            cr.restore().unwrap();
        });

        decoration.append(&drawing);

        // Content (left side)
        let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        content_box.set_margin_top(32);
        content_box.set_margin_bottom(32);
        content_box.set_margin_start(32);
        content_box.set_margin_end(150);

        let title = gtk4::Label::new(Some(crate::APP_NAME));
        title.add_css_class("title-1");
        title.add_css_class("welcome-title");
        title.set_halign(gtk4::Align::Start);

        let subtitle = gtk4::Label::new(Some("Safe and transparent system cleaning"));
        subtitle.add_css_class("title-4");
        subtitle.add_css_class("welcome-subtitle");
        subtitle.set_halign(gtk4::Align::Start);

        content_box.append(&title);
        content_box.append(&subtitle);

        overlay.set_child(Some(&content_box));
        overlay.add_overlay(&decoration);

        container.set_child(Some(&overlay));
        container.upcast()
    }

    fn create_storage_overview(&self) -> gtk4::Widget {
        let (space_card, space_label) = self.create_disk_usage_card();
        self.imp().space_label.replace(Some(space_label));
        space_card.upcast()
    }

    fn create_disk_usage_card(&self) -> (gtk4::Frame, gtk4::Label) {
        let frame = gtk4::Frame::new(None);
        frame.add_css_class("dashboard-section");
        frame.add_css_class("disk-storage-card");
        frame.set_hexpand(true);

        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
        vbox.set_margin_top(24);
        vbox.set_margin_bottom(24);
        vbox.set_margin_start(24);
        vbox.set_margin_end(24);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let icon = gtk4::Image::from_icon_name("drive-harddisk-symbolic");
        icon.add_css_class("accent");
        header.append(&icon);

        let title_label = gtk4::Label::new(Some("Disk Storage"));
        title_label.add_css_class("heading");
        header.append(&title_label);

        vbox.append(&header);

        let storage_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 28);
        storage_content.set_hexpand(true);
        storage_content.set_valign(gtk4::Align::Center);

        let chart_overlay = gtk4::Overlay::new();
        chart_overlay.set_size_request(156, 156);
        chart_overlay.set_halign(gtk4::Align::Center);
        chart_overlay.set_valign(gtk4::Align::Center);

        let donut_area = gtk4::DrawingArea::new();
        donut_area.set_size_request(156, 156);
        donut_area.set_valign(gtk4::Align::Center);
        donut_area.set_tooltip_text(Some("Disk usage for the home filesystem"));

        self.imp().total_disk_bytes.replace(0);
        self.imp().available_disk_bytes.replace(0);
        self.imp().reclaimable_bytes.replace(0);
        self.imp().has_scanned.replace(false);

        let page_weak = self.downgrade();
        donut_area.set_draw_func(move |area, cr, width, height| {
            let Some(page) = page_weak.upgrade() else {
                return;
            };
            let theme = ThemeSnapshot::from_widget(area);

            let w = width as f64;
            let h = height as f64;
            let center_x = w / 2.0;
            let center_y = h / 2.0;
            let radius = w.min(h) / 2.0 - 10.0;
            let line_width = 18.0;
            let track = theme
                .border
                .with_alpha(if theme.is_high_contrast { 0.72 } else { 0.38 });
            let (track_r, track_g, track_b, track_a) = theme::rgba_to_cairo(&track);

            let total = *page.imp().total_disk_bytes.borrow();
            let available = (*page.imp().available_disk_bytes.borrow()).min(total);
            let used = total.saturating_sub(available);
            let reclaimable = *page.imp().reclaimable_bytes.borrow();
            let has_scanned = *page.imp().has_scanned.borrow();

            let used_fraction = if total > 0 {
                used as f64 / total as f64
            } else {
                0.0
            };

            let usage_color = if used_fraction >= 0.9 {
                theme.error
            } else if used_fraction >= 0.75 {
                theme.warning
            } else {
                theme.accent_bg
            };
            let (usage_r, usage_g, usage_b, usage_a) = theme::rgba_to_cairo(&usage_color);
            let (success_r, success_g, success_b, success_a) = theme::rgba_to_cairo(&theme.success);

            cr.set_line_width(line_width);
            cr.set_line_cap(gtk4::cairo::LineCap::Round);

            // The outer ring always communicates actual disk usage.
            cr.set_source_rgba(track_r, track_g, track_b, track_a);
            cr.arc(center_x, center_y, radius, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();

            let start_angle = -std::f64::consts::PI / 2.0;
            let used_angle = used_fraction * 2.0 * std::f64::consts::PI;
            if used_angle > 0.001 {
                cr.set_source_rgba(usage_r, usage_g, usage_b, usage_a);
                cr.arc(
                    center_x,
                    center_y,
                    radius,
                    start_angle,
                    start_angle + used_angle,
                );
                let _ = cr.stroke();
            }

            // After scanning, a slim inner arc highlights how much of the used
            // space Cleaner can reclaim without obscuring the usage ring.
            if has_scanned && total > 0 {
                let reclaimable_fraction = reclaimable.min(used) as f64 / total as f64;
                let reclaimable_angle = reclaimable_fraction * 2.0 * std::f64::consts::PI;
                if reclaimable_angle > 0.001 {
                    cr.set_line_width(6.0);
                    cr.set_source_rgba(success_r, success_g, success_b, success_a);
                    cr.arc(
                        center_x,
                        center_y,
                        radius - 15.0,
                        start_angle + used_angle - reclaimable_angle,
                        start_angle + used_angle,
                    );
                    let _ = cr.stroke();
                }
            }
        });

        self.imp().donut_area.replace(Some(donut_area.clone()));
        chart_overlay.set_child(Some(&donut_area));

        let center_labels = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        center_labels.set_halign(gtk4::Align::Center);
        center_labels.set_valign(gtk4::Align::Center);

        let percent_label = gtk4::Label::new(Some("--"));
        percent_label.add_css_class("title-2");
        percent_label.set_halign(gtk4::Align::Center);
        center_labels.append(&percent_label);
        self.imp().disk_percent_label.replace(Some(percent_label));

        let percent_caption = gtk4::Label::new(Some("used"));
        percent_caption.add_css_class("caption");
        percent_caption.set_halign(gtk4::Align::Center);
        center_labels.append(&percent_caption);

        chart_overlay.add_overlay(&center_labels);
        storage_content.append(&chart_overlay);

        let details = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        details.add_css_class("disk-usage-details");
        details.set_hexpand(true);
        details.set_valign(gtk4::Align::Center);

        let summary_label = gtk4::Label::new(Some("Disk information unavailable"));
        summary_label.add_css_class("title-2");
        summary_label.set_halign(gtk4::Align::Start);
        summary_label.set_xalign(0.0);
        details.append(&summary_label);
        self.imp().disk_summary_label.replace(Some(summary_label));

        let context_label = gtk4::Label::new(None);
        context_label.add_css_class("dim-label");
        context_label.set_halign(gtk4::Align::Start);
        context_label.set_xalign(0.0);
        context_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        details.append(&context_label);
        self.imp().disk_context_label.replace(Some(context_label));

        let metrics = gtk4::Box::new(gtk4::Orientation::Horizontal, 20);
        metrics.add_css_class("disk-metrics");
        metrics.set_margin_top(14);

        let available_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        available_box.add_css_class("disk-metric");
        available_box.set_hexpand(true);

        let available_label = gtk4::Label::new(Some("--"));
        available_label.add_css_class("disk-metric-value");
        available_label.set_halign(gtk4::Align::Start);
        available_box.append(&available_label);
        self.imp()
            .disk_available_label
            .replace(Some(available_label));

        let available_caption = gtk4::Label::new(Some("Available"));
        available_caption.add_css_class("caption");
        available_caption.set_halign(gtk4::Align::Start);
        available_box.append(&available_caption);
        metrics.append(&available_box);

        let divider = gtk4::Separator::new(gtk4::Orientation::Vertical);
        metrics.append(&divider);

        let cleanup_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        cleanup_box.add_css_class("disk-metric");
        cleanup_box.set_hexpand(true);

        let value_label = gtk4::Label::new(Some("Scan to estimate"));
        value_label.add_css_class("disk-metric-value");
        value_label.set_halign(gtk4::Align::Start);
        value_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        cleanup_box.append(&value_label);

        let cleanup_caption = gtk4::Label::new(Some("Cleanup potential"));
        cleanup_caption.add_css_class("caption");
        cleanup_caption.set_halign(gtk4::Align::Start);
        cleanup_box.append(&cleanup_caption);
        metrics.append(&cleanup_box);

        details.append(&metrics);

        let activity = gtk4::Box::new(gtk4::Orientation::Horizontal, 20);
        activity.add_css_class("disk-metrics");
        activity.set_margin_top(8);

        let targets_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        targets_box.add_css_class("disk-metric");
        targets_box.set_hexpand(true);

        let targets_label = gtk4::Label::new(Some("0"));
        targets_label.add_css_class("disk-metric-value");
        targets_label.set_halign(gtk4::Align::Start);
        targets_box.append(&targets_label);
        self.imp().targets_label.replace(Some(targets_label));

        let targets_caption = gtk4::Label::new(Some("Enabled targets"));
        targets_caption.add_css_class("caption");
        targets_caption.set_halign(gtk4::Align::Start);
        targets_box.append(&targets_caption);
        activity.append(&targets_box);

        let activity_divider = gtk4::Separator::new(gtk4::Orientation::Vertical);
        activity.append(&activity_divider);

        let last_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        last_box.add_css_class("disk-metric");
        last_box.set_hexpand(true);

        let last_label = gtk4::Label::new(Some("Never"));
        last_label.add_css_class("disk-metric-value");
        last_label.set_halign(gtk4::Align::Start);
        last_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        last_box.append(&last_label);
        self.imp().last_clean_label.replace(Some(last_label));

        let last_caption = gtk4::Label::new(Some("Last cleanup"));
        last_caption.add_css_class("caption");
        last_caption.set_halign(gtk4::Align::Start);
        last_box.append(&last_caption);
        activity.append(&last_box);

        details.append(&activity);
        storage_content.append(&details);
        vbox.append(&storage_content);
        frame.set_child(Some(&vbox));

        self.refresh_disk_usage();
        (frame, value_label)
    }

    fn get_home_disk_usage() -> Option<DiskUsage> {
        let disks = Disks::new_with_refreshed_list();
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));

        // The longest matching mount path is the filesystem that actually owns
        // the home directory when /home is on a separate partition.
        let disk = disks
            .list()
            .iter()
            .filter(|disk| home_dir.starts_with(disk.mount_point()))
            .max_by_key(|disk| disk.mount_point().components().count())
            .or_else(|| disks.list().first())?;

        Some(DiskUsage {
            total: disk.total_space(),
            available: disk.available_space().min(disk.total_space()),
            device_name: disk.name().to_string_lossy().into_owned(),
            mount_point: disk.mount_point().to_path_buf(),
        })
    }

    fn refresh_disk_usage(&self) {
        if let Some(donut) = self.imp().donut_area.borrow().as_ref() {
            donut.queue_draw();
        }
        if self.imp().disk_refresh_running.replace(true) {
            return;
        }

        let (sender, receiver) = async_channel::bounded(1);
        crate::runtime().spawn_blocking(move || {
            let _ = sender.send_blocking(Self::get_home_disk_usage());
        });

        let page = self.downgrade();
        glib::spawn_future_local(async move {
            let usage = receiver.recv().await.ok().flatten();
            if let Some(page) = page.upgrade() {
                page.imp().disk_refresh_running.set(false);
                page.apply_disk_usage(usage);
            }
        });
    }

    fn apply_disk_usage(&self, usage: Option<DiskUsage>) {
        let (total, available) = usage
            .as_ref()
            .map(|usage| (usage.total, usage.available))
            .unwrap_or((0, 0));
        let used = total.saturating_sub(available);

        self.imp().total_disk_bytes.replace(total);
        self.imp().available_disk_bytes.replace(available);

        if let Some(label) = self.imp().disk_percent_label.borrow().as_ref() {
            let percentage = if total > 0 {
                used as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            let text = if total > 0 {
                format!("{percentage:.0}%")
            } else {
                "--".to_string()
            };
            label.set_text(&text);
        }

        if let Some(label) = self.imp().disk_summary_label.borrow().as_ref() {
            let text = if total > 0 {
                let used = bytesize::ByteSize(used).to_string();
                crate::i18n::tr_args("{used} used", &[("{used}", &used)])
            } else {
                crate::i18n::tr("Disk information unavailable")
            };
            label.set_text(&text);
        }

        if let Some(label) = self.imp().disk_context_label.borrow().as_ref() {
            let context = usage.as_ref().map(|usage| {
                let total = bytesize::ByteSize(usage.total).to_string();
                let mount = usage.mount_point.display().to_string();
                crate::i18n::tr_args(
                    "of {total} total on {device} (mounted at {mount})",
                    &[
                        ("{total}", &total),
                        ("{device}", &usage.device_name),
                        ("{mount}", &mount),
                    ],
                )
            });
            label.set_text(
                context
                    .as_deref()
                    .unwrap_or(&crate::i18n::tr("Home filesystem could not be detected")),
            );
        }

        if let Some(label) = self.imp().disk_available_label.borrow().as_ref() {
            let text = if total > 0 {
                bytesize::ByteSize(available).to_string()
            } else {
                "--".to_string()
            };
            label.set_text(&text);
        }

        if let Some(donut) = self.imp().donut_area.borrow().as_ref() {
            if total > 0 {
                let used = bytesize::ByteSize(used).to_string();
                let available = bytesize::ByteSize(available).to_string();
                donut.set_tooltip_text(Some(&crate::i18n::tr_args(
                    "{used} used, {available} available",
                    &[("{used}", &used), ("{available}", &available)],
                )));
            }
            donut.queue_draw();
        }
    }

    fn create_action_section(&self) -> gtk4::Widget {
        let section = gtk4::Box::new(gtk4::Orientation::Vertical, 16);

        // Action card
        let card = gtk4::Frame::new(None);
        card.add_css_class("dashboard-section");

        let inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
        inner.set_margin_top(20);
        inner.set_margin_bottom(20);
        inner.set_margin_start(20);
        inner.set_margin_end(20);

        // Info text
        let info_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        info_box.set_hexpand(true);

        // Title with icon
        let title_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        title_box.set_halign(gtk4::Align::Start);

        let icon = gtk4::Image::from_icon_name("emblem-important-symbolic");
        icon.add_css_class("accent");
        title_box.append(&icon);

        let info_title = gtk4::Label::new(Some("Start Cleanup"));
        info_title.add_css_class("heading");
        title_box.append(&info_title);

        let info_desc = gtk4::Label::new(Some(
            "Scan enabled targets or clean them using your configured confirmation settings.",
        ));
        info_desc.add_css_class("dim-label");
        info_desc.set_halign(gtk4::Align::Start);
        info_desc.set_wrap(true);
        info_desc.set_xalign(0.0);

        info_box.append(&title_box);
        info_box.append(&info_desc);

        // Buttons container
        let buttons_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        buttons_box.set_valign(gtk4::Align::Center);

        // Scan button (comes first)
        let scan_button = gtk4::Button::with_label("Scan for Files");
        scan_button.add_css_class("suggested-action");

        let page = self.downgrade();
        scan_button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                page.on_scan_clicked();
            }
        });
        self.imp().scan_button.replace(Some(scan_button.clone()));

        // Clean button
        let clean_button = gtk4::Button::with_label("Clean Now");
        clean_button.add_css_class("suggested-action");

        // Connect button
        let page = self.downgrade();
        clean_button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                page.on_clean_clicked();
            }
        });

        self.imp().clean_button.replace(Some(clean_button.clone()));

        let view_log_button = gtk4::Button::builder()
            .label("View Log")
            .tooltip_text("Review the latest cleanup details")
            .visible(false)
            .build();
        let page = self.downgrade();
        view_log_button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                page.open_cleanup_log();
            }
        });
        self.imp()
            .view_log_button
            .replace(Some(view_log_button.clone()));

        let cancel_button = gtk4::Button::with_label("Cancel Operation");
        cancel_button.set_icon_name("process-stop-symbolic");
        cancel_button.add_css_class("destructive-action");
        cancel_button.set_visible(false);
        let page = self.downgrade();
        cancel_button.connect_clicked(move |button| {
            if let Some(page) = page.upgrade() {
                page.cancel_operation(button);
            }
        });
        self.imp().cancel_button.replace(Some(cancel_button.clone()));

        buttons_box.append(&view_log_button);
        buttons_box.append(&cancel_button);
        buttons_box.append(&scan_button);
        buttons_box.append(&clean_button);

        inner.append(&info_box);
       inner.append(&buttons_box);

        card.set_child(Some(&inner));
        section.append(&card);

        section.upcast()
    }

    pub fn refresh(&self) {
        let storage = self.imp().storage.borrow();
        let storage = storage.as_ref().unwrap();

        // Update enabled targets count
        let enabled = storage.count_enabled_rules();
        if let Some(label) = self.imp().targets_label.borrow().as_ref() {
            label.set_text(&enabled.to_string());
        }

        // Update last cleanup time
        let settings = storage.get_settings();
        if let Some(label) = self.imp().last_clean_label.borrow().as_ref() {
            let text = if let Some(time) = settings.last_cleanup {
                time.format("%Y-%m-%d %H:%M").to_string()
            } else {
                "Never".to_string()
            };
            label.set_text(&text);
        }

        // The Clean button should be enabled whenever there is something to act
        // on: either rules to scan, or already-discovered files from a prior
        // scan. Without the second case the button stays disabled even after a
        // successful scan if the rule count happens to be cached as zero.
        let has_scan_items = self
            .imp()
            .scan_result
            .borrow()
            .as_ref()
            .map(|r| !r.is_empty())
            .unwrap_or(false);
        if let Some(button) = self.imp().clean_button.borrow().as_ref() {
            button.set_sensitive(enabled > 0 || has_scan_items);
        }

        // Only reset the scan-result visuals when there is no pending scan to
        // display. Otherwise refreshing on navigation would erase the user's
        // last scan output even though the underlying data is still around.
        if self.imp().scan_result.borrow().is_none() {
            if let Some(label) = self.imp().space_label.borrow().as_ref() {
                label.set_text("Scan to estimate");
            }
            self.imp().reclaimable_bytes.replace(0);
            self.imp().has_scanned.replace(false);
        }

        self.refresh_disk_usage();
    }

    fn on_scan_clicked(&self) {
        self.start_scan(false, false);
    }

    fn cancel_operation(&self, button: &gtk4::Button) {
        if let Some(cancel) = self.imp().operation_cancel.borrow().as_ref() {
            cancel.store(true, Ordering::Relaxed);
            button.set_label("Cancelling…");
            button.set_sensitive(false);
        }
    }

    fn set_operation_controls(&self, running: bool) {
        if let Some(button) = self.imp().scan_button.borrow().as_ref() {
            button.set_sensitive(!running);
        }
        if let Some(button) = self.imp().clean_button.borrow().as_ref() {
            button.set_sensitive(!running);
        }
        if let Some(button) = self.imp().cancel_button.borrow().as_ref() {
            button.set_visible(running);
            button.set_sensitive(running);
            button.set_label("Cancel Operation");
        }
    }

    fn start_scan(&self, clean_after_scan: bool, automatic: bool) -> bool {
        if let Some(window) = self
            .root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok())
            .and_then(|window| window.downcast::<crate::ui::MainWindow>().ok())
        {
            if window.storage_analyzer_operation_is_running()
                || window.system_operation_is_running()
            {
                if !automatic {
                    self.show_info_dialog(
                        "Operation in Progress",
                        "Cancel the active scan or wait for the administrator operation to finish before cleaning.",
                    );
                }
                return false;
            }
        }

        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();
        let settings = storage.get_settings();

        // Guard against concurrent operations
        let Some(running_flag) = self.imp().operation_running.borrow().as_ref().cloned() else {
            return false;
        };
        if running_flag
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            if !automatic {
                self.show_info_dialog("Operation in Progress", "A scan or cleanup is already running. Please wait for it to complete.");
            }
            return false;
        }

        // Update UI
        if let Some(label) = self.imp().space_label.borrow().as_ref() {
            label.set_text("Scanning...");
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        self.imp()
            .operation_cancel
            .replace(Some(cancellation.clone()));
        self.set_operation_controls(true);

        // Run scan in background
        let (sender, receiver) = async_channel::bounded::<ScanResult>(1);

        let browser_rules = storage.get_browser_rules();
        let app_rules = storage.get_app_rules();
        let custom_rules = storage.get_custom_rules();
        let system_rules = storage.get_system_rules();
        let scan_options = ScanOptions {
            log_retention_days: settings.log_retention_days,
        };
        let scan_application_logs = settings.application_log_cleanup_enabled;

        crate::runtime().spawn_blocking(move || {
            let scanner = Scanner::with_options_and_cancellation(scan_options, cancellation);
            let mut total_result = ScanResult::new();

            total_result.merge(scanner.scan_browser_rules(&browser_rules));
            total_result.merge(scanner.scan_app_rules(&app_rules));
            total_result.merge(scanner.scan_custom_rules(&custom_rules));
            total_result.merge(scanner.scan_system_rules(&system_rules));
            if scan_application_logs {
                total_result.merge(scanner.scan_application_logs());
            }

            let _ = sender.send_blocking(total_result);
        });

        // Handle result
        let page = self.clone();
        glib::spawn_future_local(async move {
            let mut continue_to_clean = false;
            if let Ok(result) = receiver.recv().await {
                if result.cancelled {
                    if let Some(label) = page.imp().space_label.borrow().as_ref() {
                        label.set_text("Scan cancelled");
                    }
                    page.imp().scan_result.replace(None);
                    page.imp().reclaimable_bytes.replace(0);
                    page.imp().has_scanned.replace(false);
                } else {
                // Update the label with formatted size
                if let Some(label) = page.imp().space_label.borrow().as_ref() {
                    label.set_text(&result.formatted_size());
                }

                // Update donut chart
                page.imp().reclaimable_bytes.replace(result.total_size);
                page.imp().has_scanned.replace(true);
                page.refresh_disk_usage();

                let has_items = !result.is_empty();
                page.imp().scan_result.replace(Some(result));

                // Enable the Clean button so the user can act on the results
                // they just saw. The previous code only set sensitivity at
                // construction time, so the button stayed disabled even when
                // a scan succeeded.
                if has_items {
                    if let Some(button) = page.imp().clean_button.borrow().as_ref() {
                        button.set_sensitive(true);
                    }
                }
                continue_to_clean = clean_after_scan;
                }
            }
            page.imp().operation_cancel.replace(None);
            running_flag.store(false, Ordering::SeqCst);
            page.set_operation_controls(false);
            if continue_to_clean {
                if automatic {
                    page.on_automatic_clean_ready();
                } else {
                    page.on_clean_clicked();
                }
            }
        });

        true
    }

    pub fn request_clean(&self) {
        // Tray cleanup always starts from a fresh scan so it cannot act on a
        // stale preview created before the selected rules changed.
        self.start_scan(true, false);
    }

    pub fn request_scheduled_clean(&self) -> bool {
        // Automatic cleanup is explicitly enabled in Settings, so it skips
        // interactive dialogs while retaining all scan and safety checks.
        self.start_scan(true, true)
    }

    pub fn operation_is_running(&self) -> bool {
        self.imp()
            .operation_running
            .borrow()
            .as_ref()
            .map(|flag| flag.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    pub fn show_operation_running_dialog(&self) {
        self.show_info_dialog(
            "Operation in Progress",
            "Wait for the current scan or cleanup to finish before closing Data Cleaner.",
        );
    }

    fn on_clean_clicked(&self) {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();
        let settings = storage.get_settings();

        // Check if we have scan results
        let scan_result = self.imp().scan_result.borrow().clone();

        if scan_result.is_none() {
            // Scan first, then continue through the normal confirmation flow.
            self.start_scan(true, false);
            return;
        }

        let scan_result = scan_result.unwrap();

        if scan_result.is_empty() && !settings.system_journal_cleanup_enabled {
            self.show_info_dialog("Nothing to Clean", "No files were found to clean with the current settings.");
            return;
        }

        // Show confirmation dialog
        if settings.confirm_before_clean {
            self.show_confirmation_dialog(&scan_result);
        } else {
            self.execute_clean(&scan_result, true);
        }
    }

    fn on_automatic_clean_ready(&self) {
        let Some(scan_result) = self.imp().scan_result.borrow().clone() else {
            return;
        };

        if scan_result.is_empty() {
            tracing::info!("Scheduled cleanup found nothing to delete");
            self.imp().scan_result.replace(None);
            self.refresh();
            return;
        }

        self.execute_clean(&scan_result, false);
    }

    fn show_confirmation_dialog(&self, scan_result: &ScanResult) {
        let settings = self
            .imp()
            .storage
            .borrow()
            .as_ref()
            .unwrap()
            .get_settings();
        let journal_notice = if settings.system_journal_cleanup_enabled {
            crate::i18n::tr_args(
                "\nThe archived system journal older than {days} days will also be vacuumed with administrator approval.",
                &[("{days}", &settings.log_retention_days.to_string())],
            )
        } else {
            String::new()
        };
        let window = self.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
        let dialog = adw::MessageDialog::new(
            window.as_ref(),
            Some(&crate::i18n::tr("Confirm Cleanup")),
            Some(&crate::i18n::tr_args(
                "This will delete {count} files ({size}).{journal}\n\nThis action cannot be undone.",
                &[
                    ("{count}", &scan_result.file_count.to_string()),
                    ("{size}", &scan_result.formatted_size()),
                    ("{journal}", &journal_notice),
                ],
            )),
        );

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("clean", "Clean");
        dialog.set_response_appearance("clean", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let page = self.downgrade();
        let result = scan_result.clone();
        dialog.connect_response(None, move |_: &adw::MessageDialog, response| {
            if response == "clean" {
                if let Some(page) = page.upgrade() {
                    page.execute_clean(&result, true);
                }
            }
        });

        crate::i18n::translate_widget_tree(&dialog);
        dialog.present();
    }

    fn execute_clean(&self, scan_result: &ScanResult, interactive: bool) {
        let storage = self.imp().storage.borrow().as_ref().unwrap().clone();
        let settings = storage.get_settings();
        let clean_options = CleanOptions {
            max_files_per_operation: settings.max_files_per_operation,
            max_size_per_operation: settings.max_size_per_operation,
        };

        // Guard against concurrent operations
        let Some(running_flag) = self.imp().operation_running.borrow().as_ref().cloned() else {
            return;
        };
        if running_flag
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            if interactive {
                self.show_info_dialog("Operation in Progress", "A scan or cleanup is already running. Please wait for it to complete.");
            }
            return;
        }

        if let Err(reason) = Cleaner::with_options(clean_options).validate_operation(scan_result) {
            let mut blocked_result = crate::models::CleanResult::new();
            blocked_result.blocked(reason.clone());
            self.store_cleanup_log(
                &blocked_result,
                scan_result,
                None,
                !interactive,
                settings.system_journal_cleanup_enabled,
            );
            if interactive {
                self.show_cleanup_result_dialog("Cleanup Blocked", &reason);
            } else {
                tracing::warn!("Scheduled cleanup blocked: {}", reason);
            }
            running_flag.store(false, Ordering::SeqCst);
            self.set_operation_controls(false);
            return;
        }

        let cancellation = Arc::new(AtomicBool::new(false));
        self.imp()
            .operation_cancel
            .replace(Some(cancellation.clone()));
        self.set_operation_controls(true);

        let (sender, receiver) = async_channel::bounded(1);
        let result = scan_result.clone();
        let log_scan_result = scan_result.clone();
        let show_summary = interactive && settings.show_cleanup_summary;
        // System-journal cleanup requires an interactive PolicyKit prompt and
        // is therefore intentionally skipped by scheduled background runs.
        let clean_system_journal = interactive && settings.system_journal_cleanup_enabled;
        let log_retention_days = settings.log_retention_days;

        crate::runtime().spawn_blocking(move || {
            let cleaner = Cleaner::with_options_and_cancellation(clean_options, cancellation);
            let clean_result = cleaner.clean(&result);
            let journal_result = if clean_system_journal && !clean_result.cancelled {
                Some(vacuum_system_journal(log_retention_days))
            } else {
                None
            };
            let _ = sender.send_blocking((clean_result, journal_result));
        });

        let page = self.clone();
        glib::spawn_future_local(async move {
            if let Ok((result, journal_result)) = receiver.recv().await {
                if let Some(reason) = result.blocked_reason.as_deref() {
                    page.store_cleanup_log(
                        &result,
                        &log_scan_result,
                        journal_result.as_ref(),
                        !interactive,
                        settings.system_journal_cleanup_enabled,
                    );
                    if interactive {
                        page.show_cleanup_result_dialog("Cleanup Blocked", reason);
                    } else {
                        tracing::warn!("Scheduled cleanup blocked: {}", reason);
                    }
                    running_flag.store(false, Ordering::SeqCst);
                    page.imp().operation_cancel.replace(None);
                    page.set_operation_controls(false);
                    return;
                }

                // Update last cleanup time
                if let Err(e) = storage.update_settings(|s| {
                    s.last_cleanup = Some(chrono::Utc::now());
                }) {
                    tracing::warn!("Failed to update last cleanup time: {}", e);
                }

                page.store_cleanup_log(
                    &result,
                    &log_scan_result,
                    journal_result.as_ref(),
                    !interactive,
                    settings.system_journal_cleanup_enabled,
                );

                let journal_failed = matches!(&journal_result, Some(Err(_)));
                let journal_note = match &journal_result {
                    Some(Ok(message)) => crate::i18n::tr_args(
                        "\nSystem journal: {message}.",
                        &[("{message}", message)],
                    ),
                    Some(Err(error)) => crate::i18n::tr_args(
                        "\nSystem journal failed: {error}",
                        &[("{error}", error)],
                    ),
                    None => String::new(),
                };
                let total_failures = result.failed_count() + usize::from(journal_failed);

                if interactive
                    && (show_summary || total_failures > 0 || result.cancelled)
                {
                    page.show_cleanup_result_dialog(
                        if result.cancelled {
                            "Cleanup Cancelled"
                        } else {
                            "Cleanup Complete"
                        },
                        &crate::i18n::tr_args(
                            "Deleted {count} files and freed {size}.\n{failed} items failed.{journal}",
                            &[
                                ("{count}", &result.files_deleted_count().to_string()),
                                ("{size}", &result.formatted_bytes_freed()),
                                ("{failed}", &total_failures.to_string()),
                                ("{journal}", &journal_note),
                            ],
                        ),
                    );
                } else if total_failures > 0 || result.cancelled {
                    tracing::warn!(
                        "Scheduled cleanup finished with {} failures (cancelled: {})",
                        total_failures,
                        result.cancelled
                    );
                }

                // Refresh
                page.imp().scan_result.replace(None);
                page.refresh();
            }
            page.imp().operation_cancel.replace(None);
            running_flag.store(false, Ordering::SeqCst);
            page.set_operation_controls(false);
        });
    }

    fn store_cleanup_log(
        &self,
        clean_result: &crate::models::CleanResult,
        scan_result: &ScanResult,
        journal_result: Option<&Result<String, String>>,
        automatic: bool,
        system_journal_enabled: bool,
    ) {
        if let Some(window) = self
            .root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok())
            .and_then(|window| window.downcast::<crate::ui::MainWindow>().ok())
        {
            window.set_cleanup_log(
                clean_result,
                scan_result,
                journal_result,
                automatic,
                system_journal_enabled,
            );
            if let Some(button) = self.imp().view_log_button.borrow().as_ref() {
                button.set_visible(true);
            }
        }
    }

    fn open_cleanup_log(&self) {
        if let Some(window) = self
            .root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok())
            .and_then(|window| window.downcast::<crate::ui::MainWindow>().ok())
        {
            window.navigate_to_cleanup_log();
        }
    }

    fn show_cleanup_result_dialog(&self, title: &str, message: &str) {
        let window = self.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
        let dialog = adw::MessageDialog::new(
            window.as_ref(),
            Some(&crate::i18n::tr(title)),
            Some(message),
        );

        dialog.add_response("log", "View Log");
        dialog.add_response("ok", "OK");
        dialog.set_default_response(Some("ok"));
        dialog.set_close_response("ok");

        let page = self.downgrade();
        dialog.connect_response(None, move |_, response| {
            if response == "log" {
                if let Some(page) = page.upgrade() {
                    page.open_cleanup_log();
                }
            }
        });
        crate::i18n::translate_widget_tree(&dialog);
        dialog.present();
    }

    fn show_info_dialog(&self, title: &str, message: &str) {
        let window = self.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
        let dialog = adw::MessageDialog::new(
            window.as_ref(),
            Some(&crate::i18n::tr(title)),
            Some(&crate::i18n::tr(message)),
        );

        dialog.add_response("ok", "OK");
        dialog.set_default_response(Some("ok"));
        crate::i18n::translate_widget_tree(&dialog);
        dialog.present();
    }
}
