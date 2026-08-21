use crate::i18n::{tr, tr_args};
use crate::models::{CleanResult, ScanResult};
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gdk, glib};
use std::cell::RefCell;
use std::fmt::Write;

const MAX_LOG_DETAIL_ITEMS: usize = 5_000;
const MAX_LOG_DETAIL_BYTES: usize = 2 * 1024 * 1024;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct CleanupLogPage {
        pub completed_label: RefCell<Option<gtk4::Label>>,
        pub summary_label: RefCell<Option<gtk4::Label>>,
        pub log_buffer: RefCell<Option<gtk4::TextBuffer>>,
        pub log_text: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CleanupLogPage {
        const NAME: &'static str = "DataCleanerCleanupLogPage";
        type Type = super::CleanupLogPage;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for CleanupLogPage {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for CleanupLogPage {}
    impl BoxImpl for CleanupLogPage {}
}

glib::wrapper! {
    pub struct CleanupLogPage(ObjectSubclass<imp::CleanupLogPage>)
        @extends gtk4::Widget, gtk4::Box;
}

impl CleanupLogPage {
    pub fn new() -> Self {
        let page: Self = glib::Object::builder()
            .property("orientation", gtk4::Orientation::Vertical)
            .property("spacing", 16)
            .build();
        page.setup_ui();
        page
    }

    fn setup_ui(&self) {
        self.set_margin_top(16);
        self.set_margin_bottom(20);
        self.set_margin_start(24);
        self.set_margin_end(24);

        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);

        let back_button = gtk4::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back to Dashboard")
            .build();
        back_button.add_css_class("flat");

        let page = self.downgrade();
        back_button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                if let Some(window) = page
                    .root()
                    .and_then(|root| root.downcast::<gtk4::Window>().ok())
                    .and_then(|window| window.downcast::<crate::ui::MainWindow>().ok())
                {
                    window.navigate_to_dashboard();
                }
            }
        });
        actions.append(&back_button);

        let heading_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        heading_box.set_hexpand(true);

        let heading = gtk4::Label::new(Some("Latest Cleanup"));
        heading.add_css_class("title-2");
        heading.set_halign(gtk4::Align::Start);
        heading_box.append(&heading);

        let completed_label = gtk4::Label::new(None);
        completed_label.add_css_class("dim-label");
        completed_label.set_halign(gtk4::Align::Start);
        heading_box.append(&completed_label);
        actions.append(&heading_box);

        let copy_button = gtk4::Button::builder()
            .label("Copy Log")
            .tooltip_text("Copy the complete cleanup log")
            .build();
        let page = self.downgrade();
        copy_button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                if let Some(display) = gdk::Display::default() {
                    display
                        .clipboard()
                        .set_text(page.imp().log_text.borrow().as_str());
                }
            }
        });
        actions.append(&copy_button);
        self.append(&actions);

        let summary_label = gtk4::Label::new(None);
        summary_label.add_css_class("cleanup-log-summary");
        summary_label.set_halign(gtk4::Align::Start);
        summary_label.set_wrap(true);
        summary_label.set_xalign(0.0);
        self.append(&summary_label);

        let log_view = gtk4::TextView::new();
        log_view.set_editable(false);
        log_view.set_cursor_visible(false);
        log_view.set_monospace(true);
        log_view.set_wrap_mode(gtk4::WrapMode::None);
        log_view.set_left_margin(14);
        log_view.set_right_margin(14);
        log_view.set_top_margin(12);
        log_view.set_bottom_margin(12);
        log_view.add_css_class("cleanup-log-view");

        let log_buffer = log_view.buffer();
        self.imp().log_buffer.replace(Some(log_buffer));

        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_hexpand(true);
        scrolled.set_vexpand(true);
        scrolled.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Automatic);
        scrolled.set_child(Some(&log_view));

        let log_frame = gtk4::Frame::new(None);
        log_frame.add_css_class("cleanup-log-frame");
        log_frame.set_hexpand(true);
        log_frame.set_vexpand(true);
        log_frame.set_child(Some(&scrolled));
        self.append(&log_frame);

        self.imp().completed_label.replace(Some(completed_label));
        self.imp().summary_label.replace(Some(summary_label));
    }

    pub fn set_result(
        &self,
        clean_result: &CleanResult,
        scan_result: &ScanResult,
        journal_result: Option<&Result<String, String>>,
        automatic: bool,
        system_journal_enabled: bool,
    ) {
        let completed_at = chrono::Local::now();
        let mode = if automatic {
            tr("Scheduled")
        } else {
            tr("Manual")
        };
        let journal_failed = matches!(journal_result, Some(Err(_)));
        let issue_count = clean_result.failed.len()
            + clean_result.skipped.len()
            + scan_result.skipped.len()
            + scan_result.security_violations.len()
            + scan_result.errors.len()
            + usize::from(journal_failed);

        let status = if clean_result.blocked_reason.is_some() {
            tr("Blocked")
        } else if clean_result.cancelled {
            tr("Cancelled")
        } else if issue_count > 0 {
            tr("Completed with notices")
        } else {
            tr("Completed successfully")
        };

        if let Some(label) = self.imp().completed_label.borrow().as_ref() {
            label.set_text(&tr_args(
                "{mode} cleanup · {time}",
                &[
                    ("{mode}", &mode),
                    ("{time}", &completed_at.format("%Y-%m-%d %H:%M:%S").to_string()),
                ],
            ));
        }

        if let Some(label) = self.imp().summary_label.borrow().as_ref() {
            label.set_text(&tr_args(
                "{status} · {files} files and {folders} folders removed · {size} freed · {notices} notices",
                &[
                    ("{status}", &status),
                    ("{files}", &clean_result.deleted_files.len().to_string()),
                    ("{folders}", &clean_result.deleted_directories.len().to_string()),
                    ("{size}", &clean_result.formatted_bytes_freed()),
                    ("{notices}", &issue_count.to_string()),
                ],
            ));
        }

        let mut log = String::new();
        let mut detail_budget = MAX_LOG_DETAIL_ITEMS;
        let _ = writeln!(log, "{}", tr("DATA CLEANER — CLEANUP LOG"));
        let completed = completed_at.format("%Y-%m-%d %H:%M:%S %:z").to_string();
        let _ = writeln!(
            log,
            "{}",
            tr_args("Completed: {time}", &[("{time}", &completed)])
        );
        let _ = writeln!(
            log,
            "{}",
            tr_args("Mode: {mode}", &[("{mode}", &mode)])
        );
        let _ = writeln!(
            log,
            "{}",
            tr_args("Status: {status}", &[("{status}", &status)])
        );
        let _ = writeln!(
            log,
            "{}",
            tr_args(
                "Space freed: {size}",
                &[("{size}", &clean_result.formatted_bytes_freed())],
            )
        );
        let _ = writeln!(
            log,
            "{}",
            tr_args(
                "Deleted files: {count}",
                &[("{count}", &clean_result.deleted_files.len().to_string())],
            )
        );
        let _ = writeln!(
            log,
            "{}",
            tr_args(
                "Deleted folders: {count}",
                &[("{count}", &clean_result.deleted_directories.len().to_string())],
            )
        );
        let _ = writeln!(
            log,
            "{}",
            tr_args(
                "Failed items: {count}",
                &[("{count}", &clean_result.failed.len().to_string())],
            )
        );
        let _ = writeln!(
            log,
            "{}",
            tr_args(
                "Skipped items: {count}",
                &[(
                    "{count}",
                    &(clean_result.skipped.len() + scan_result.skipped.len()).to_string(),
                )],
            )
        );

        if let Some(reason) = clean_result.blocked_reason.as_deref() {
            let _ = writeln!(
                log,
                "{}",
                tr_args("\nBLOCKED\n  {reason}", &[("{reason}", reason)])
            );
        }

        Self::append_paths(
            &mut log,
            &tr("DELETED FILES"),
            &tr("DELETED"),
            &clean_result.deleted_files,
            &mut detail_budget,
        );
        Self::append_paths(
            &mut log,
            &tr("DELETED EMPTY FOLDERS"),
            &tr("DELETED"),
            &clean_result.deleted_directories,
            &mut detail_budget,
        );
        Self::append_path_reasons(
            &mut log,
            &tr("SKIPPED DURING CLEANUP"),
            &tr("SKIPPED"),
            &clean_result.skipped,
            &mut detail_budget,
        );
        Self::append_path_reasons(
            &mut log,
            &tr("SKIPPED DURING SCAN"),
            &tr("SKIPPED"),
            &scan_result.skipped,
            &mut detail_budget,
        );
        Self::append_path_reasons(
            &mut log,
            &tr("FAILED"),
            &tr("FAILED"),
            &clean_result.failed,
            &mut detail_budget,
        );
        Self::append_messages(
            &mut log,
            &tr("SECURITY NOTICES"),
            &tr("NOTICE"),
            &scan_result.security_violations,
            &mut detail_budget,
        );
        Self::append_messages(
            &mut log,
            &tr("SCAN ERRORS"),
            &tr("ERROR"),
            &scan_result.errors,
            &mut detail_budget,
        );

        let _ = writeln!(log, "{}", tr("\nSYSTEM JOURNAL"));
        match journal_result {
            Some(Ok(message)) => {
                let _ = writeln!(
                    log,
                    "{}",
                    tr_args("  [CLEANED] {message}", &[("{message}", message)])
                );
            }
            Some(Err(error)) => {
                let _ = writeln!(
                    log,
                    "{}",
                    tr_args("  [FAILED] {error}", &[("{error}", error)])
                );
            }
            None if system_journal_enabled && automatic => {
                let _ = writeln!(
                    log,
                    "{}",
                    tr(
                        "  [SKIPPED] Scheduled cleanup cannot request administrator approval",
                    )
                );
            }
            None if system_journal_enabled && clean_result.cancelled => {
                let _ = writeln!(
                    log,
                    "{}",
                    tr("  [SKIPPED] Cleanup was cancelled")
                );
            }
            None if system_journal_enabled && clean_result.blocked_reason.is_some() => {
                let _ = writeln!(
                    log,
                    "{}",
                    tr("  [SKIPPED] Cleanup was blocked")
                );
            }
            None if system_journal_enabled => {
                let _ = writeln!(
                    log,
                    "{}",
                    tr("  [NOT RUN] No journal result was returned")
                );
            }
            None => {
                let _ = writeln!(
                    log,
                    "{}",
                    tr("  [DISABLED] System journal cleanup was not enabled")
                );
            }
        }

        if let Some(buffer) = self.imp().log_buffer.borrow().as_ref() {
            buffer.set_text(&log);
        }
        self.imp().log_text.replace(log);
    }

    fn append_paths(
        log: &mut String,
        heading: &str,
        status: &str,
        paths: &[std::path::PathBuf],
        budget: &mut usize,
    ) {
        let _ = writeln!(
            log,
            "{}",
            tr_args(
                "\n{heading} ({count})",
                &[
                    ("{heading}", &tr(heading)),
                    ("{count}", &paths.len().to_string()),
                ],
            )
        );
        if paths.is_empty() {
            let _ = writeln!(log, "{}", tr("  None"));
            return;
        }
        let status = tr(status);
        let mut shown = 0;
        for path in paths.iter().take(*budget) {
            let line = tr_args(
                "  [{status}] {path}\n",
                &[
                    ("{status}", &status),
                    ("{path}", &path.display().to_string()),
                ],
            );
            if !Self::push_detail_line(log, &line) {
                *budget = 0;
                break;
            }
            shown += 1;
            *budget -= 1;
        }
        Self::append_omitted(log, paths.len().saturating_sub(shown));
    }

    fn append_path_reasons(
        log: &mut String,
        heading: &str,
        status: &str,
        entries: &[(std::path::PathBuf, String)],
        budget: &mut usize,
    ) {
        let _ = writeln!(
            log,
            "{}",
            tr_args(
                "\n{heading} ({count})",
                &[
                    ("{heading}", &tr(heading)),
                    ("{count}", &entries.len().to_string()),
                ],
            )
        );
        if entries.is_empty() {
            let _ = writeln!(log, "{}", tr("  None"));
            return;
        }
        let status = tr(status);
        let mut shown = 0;
        for (path, reason) in entries.iter().take(*budget) {
            let line = tr_args(
                "  [{status}] {path}\n    Reason: {reason}\n",
                &[
                    ("{status}", &status),
                    ("{path}", &path.display().to_string()),
                    ("{reason}", reason),
                ],
            );
            if !Self::push_detail_line(log, &line) {
                *budget = 0;
                break;
            }
            shown += 1;
            *budget -= 1;
        }
        Self::append_omitted(log, entries.len().saturating_sub(shown));
    }

    fn append_messages(
        log: &mut String,
        heading: &str,
        status: &str,
        entries: &[String],
        budget: &mut usize,
    ) {
        let _ = writeln!(
            log,
            "{}",
            tr_args(
                "\n{heading} ({count})",
                &[
                    ("{heading}", &tr(heading)),
                    ("{count}", &entries.len().to_string()),
                ],
            )
        );
        if entries.is_empty() {
            let _ = writeln!(log, "{}", tr("  None"));
            return;
        }
        let status = tr(status);
        let mut shown = 0;
        for message in entries.iter().take(*budget) {
            let line = tr_args(
                "  [{status}] {message}\n",
                &[("{status}", &status), ("{message}", message)],
            );
            if !Self::push_detail_line(log, &line) {
                *budget = 0;
                break;
            }
            shown += 1;
            *budget -= 1;
        }
        Self::append_omitted(log, entries.len().saturating_sub(shown));
    }

    fn push_detail_line(log: &mut String, line: &str) -> bool {
        if log.len().saturating_add(line.len()) > MAX_LOG_DETAIL_BYTES {
            return false;
        }
        log.push_str(line);
        true
    }

    fn append_omitted(log: &mut String, omitted: usize) {
        if omitted > 0 {
            let _ = writeln!(
                log,
                "{}",
                tr_args(
                    "  [TRUNCATED] {omitted} additional entries omitted from the in-app log",
                    &[("{omitted}", &omitted.to_string())],
                )
            );
        }
    }
}

impl Default for CleanupLogPage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_log_is_bounded_by_item_count() {
        let paths: Vec<_> = (0..MAX_LOG_DETAIL_ITEMS + 20)
            .map(|index| std::path::PathBuf::from(format!("/tmp/cache/{index}")))
            .collect();
        let mut log = String::new();
        let mut budget = MAX_LOG_DETAIL_ITEMS;

        CleanupLogPage::append_paths(&mut log, "FILES", "DELETED", &paths, &mut budget);

        assert_eq!(budget, 0);
        assert!(log.contains("20 additional entries omitted"));
        assert!(log.len() <= MAX_LOG_DETAIL_BYTES + 1_024);
    }
}
