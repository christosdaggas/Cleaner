use crate::models::{
    AppRule, BrowserDataType, BrowserRule, BrowserType, CustomRule, DeletionMode, FileEntry,
    ScanResult, SystemRule,
};
use crate::services::SecurityAuditor;
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    pub log_retention_days: u32,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            log_retention_days: 30,
        }
    }
}

pub struct Scanner {
    security: SecurityAuditor,
    log_retention_days: u32,
    cancelled: Arc<AtomicBool>,
}

impl Scanner {
    pub fn new() -> Self {
        Self::with_options(ScanOptions::default())
    }

    pub fn with_options(options: ScanOptions) -> Self {
        Self::with_options_and_cancellation(options, Arc::new(AtomicBool::new(false)))
    }

    pub fn with_options_and_cancellation(
        options: ScanOptions,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        let security = SecurityAuditor::new();
        // Following directory symlinks allows a lexical cleanup path to escape
        // its scanned root. This unsafe mode is intentionally disabled even
        // when loading an older configuration that requested it.
        Self {
            security,
            log_retention_days: options.log_retention_days.max(1),
            cancelled,
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn scan_browser_rules(&self, rules: &[BrowserRule]) -> ScanResult {
        // Pre-compute which browsers are currently running to prevent
        // cleaning SQLite databases (History, Cookies) while in use.
        // `running_browsers()` walks /proc exactly once; calling
        // `is_running()` per browser would walk it nine times.
        let running_browsers: HashSet<BrowserType> = BrowserType::running_browsers();
        let installed_browsers: HashSet<BrowserType> = BrowserType::all()
            .iter()
            .copied()
            .filter(BrowserType::is_installed)
            .collect();

        let results: Vec<ScanResult> = rules
            .par_iter()
            .filter(|r| r.enabled)
            .map(|rule| {
                let mut result = ScanResult::new();

                if self.is_cancelled() {
                    result.cancelled = true;
                    return result;
                }

                if rule.custom_path.is_none() && !installed_browsers.contains(&rule.browser) {
                    if let Some(path) = rule.effective_path() {
                        result.add_skipped(
                            path,
                            format!("{} is not installed", rule.browser.display_name()),
                        );
                    }
                    return result;
                }

                // Skip non-cache data for running browsers to prevent SQLite corruption
                if rule.data_type != BrowserDataType::Cache
                    && running_browsers.contains(&rule.browser)
                {
                    if let Some(path) = rule.effective_path() {
                        result.add_skipped(
                            path,
                            format!(
                                "{} is currently running \u{2014} close it before cleaning {} to avoid data corruption",
                                rule.browser.display_name(),
                                rule.data_type.display_name()
                            ),
                        );
                    }
                    return result;
                }

                if let Some(reason) = Self::unsupported_browser_cleanup_reason(rule.browser, rule.data_type) {
                    result.add_skipped(PathBuf::from(rule.display_name()), reason);
                    return result;
                }

                let Some(path) = rule.effective_path() else {
                    result.add_error(format!(
                        "No path for browser rule: {}",
                        rule.display_name()
                    ));
                    return result;
                };

                for relative_path in rule.data_type.relative_paths(rule.browser) {
                    let pattern = path.join(relative_path);
                    result.merge(
                        self.scan_browser_pattern(
                            &pattern,
                            rule.data_type.recurse_directories(),
                        ),
                    );
                }
                result
            })
            .collect();

        let mut result = ScanResult::new();
        let mut seen_files = HashSet::new();
        let mut seen_dirs = HashSet::new();
        for mut r in results {
            for file in r.files.drain(..) {
                if seen_files.insert(file.path.clone()) {
                    result.add_file(file);
                }
            }
            for dir in r.directories.drain(..) {
                if seen_dirs.insert(dir.path.clone()) {
                    result.add_directory(dir);
                }
            }
            result.skipped.extend(r.skipped);
            result.security_violations.extend(r.security_violations);
            result.errors.extend(r.errors);
            result.cancelled |= r.cancelled;
        }
        result.cancelled |= self.is_cancelled();
        result
    }

    fn unsupported_browser_cleanup_reason(
        browser: BrowserType,
        data_type: BrowserDataType,
    ) -> Option<&'static str> {
        match (browser, data_type) {
            (BrowserType::Firefox, BrowserDataType::History | BrowserDataType::DownloadHistory) => {
                Some(
                    "Firefox stores history, downloads, and bookmarks in places.sqlite; whole-file deletion is disabled until SQLite-level cleanup is implemented",
                )
            }
            (
                BrowserType::Chrome
                | BrowserType::Chromium
                | BrowserType::Brave
                | BrowserType::Edge
                | BrowserType::Opera
                | BrowserType::Vivaldi
                | BrowserType::Yandex
                | BrowserType::DuckDuckGo,
                BrowserDataType::DownloadHistory,
            ) => Some(
                "Chromium-based browsers store download history inside the History database; download-only cleanup is disabled until SQLite-level cleanup is implemented",
            ),
            _ => None,
        }
    }

    pub fn scan_app_rules(&self, rules: &[AppRule]) -> ScanResult {
        let results: Vec<ScanResult> = rules
            .par_iter()
            .filter(|r| r.enabled)
            .map(|rule| {
                let mut result = ScanResult::new();

                if self.is_cancelled() {
                    result.cancelled = true;
                    return result;
                }

                let Some(path) = rule.expanded_path() else {
                    result.add_error(format!("Cannot expand path for rule: {}", rule.name));
                    return result;
                };

                if rule.path.to_string_lossy().contains('*') {
                    result.merge(self.scan_glob_pattern(&rule.path));
                } else {
                    result.merge(self.scan_directory(&path, false));
                }
                result
            })
            .collect();

        let mut result = ScanResult::new();
        for r in results {
            result.merge(r);
        }
        result.cancelled |= self.is_cancelled();
        result
    }

    pub fn scan_custom_rules(&self, rules: &[CustomRule]) -> ScanResult {
        let results: Vec<ScanResult> = rules
            .par_iter()
            .filter(|r| r.enabled)
            .map(|rule| {
                let mut result = ScanResult::new();

                if self.is_cancelled() {
                    result.cancelled = true;
                    return result;
                }

                let Some(path) = rule.expanded_path() else {
                    result.add_error(format!("Cannot expand path for rule: {}", rule.name));
                    return result;
                };

                let include_dirs = matches!(rule.deletion_mode, DeletionMode::FilesAndDirectories);
                result.merge(self.scan_directory_with_options(
                    &path,
                    include_dirs,
                    rule.file_pattern.as_deref(),
                    rule.min_age_days,
                ));
                result
            })
            .collect();

        let mut result = ScanResult::new();
        for r in results {
            result.merge(r);
        }
        result.cancelled |= self.is_cancelled();
        result
    }

    pub fn scan_system_rules(&self, rules: &[SystemRule]) -> ScanResult {
        let results: Vec<ScanResult> = rules
            .par_iter()
            .filter(|r| r.enabled)
            .map(|rule| {
                let mut result = ScanResult::new();

                if self.is_cancelled() {
                    result.cancelled = true;
                    return result;
                }

                if rule.requires_root() {
                    result.add_skipped(
                        PathBuf::from(rule.display_name()),
                        "Requires root privileges \u{2014} use the System page to run with administrator access",
                    );
                    return result;
                }

                for path in rule.effective_paths() {
                    if self.is_cancelled() {
                        result.cancelled = true;
                        break;
                    }
                    result.merge(self.scan_directory(&path, false));
                }
                result
            })
            .collect();

        let mut result = ScanResult::new();
        for r in results {
            result.merge(r);
        }
        result.cancelled |= self.is_cancelled();
        result
    }

    /// Scan user-owned application logs without touching `/var/log` or the
    /// active system journal. Only conventional `.log` and rotated
    /// `.log.*` files older than the configured retention period qualify.
    pub fn scan_application_logs(&self) -> ScanResult {
        let mut roots = Vec::new();
        if let Some(state) = dirs::state_dir() {
            roots.push(state);
        }
        if let Some(cache) = dirs::cache_dir() {
            roots.push(cache);
        }
        self.scan_application_log_paths(&roots, self.log_retention_days)
    }

    fn scan_application_log_paths(&self, roots: &[PathBuf], retention_days: u32) -> ScanResult {
        let mut result = ScanResult::new();
        let Some(cutoff) = std::time::SystemTime::now().checked_sub(
            std::time::Duration::from_secs(retention_days.max(1) as u64 * 86_400),
        ) else {
            result.add_error("Could not calculate the application-log retention cutoff");
            return result;
        };

        for root in roots {
            if self.is_cancelled() {
                result.cancelled = true;
                break;
            }
            if !root.exists() {
                continue;
            }

            let root_audit = self.security.audit(root);
            if !root_audit.is_safe {
                for violation in root_audit.violations {
                    result.add_security_violation(violation.to_string());
                }
                continue;
            }

            for entry in WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
            {
                if self.is_cancelled() {
                    result.cancelled = true;
                    break;
                }

                let path = entry.path();
                if path == root || !entry.file_type().is_file() || entry.file_type().is_symlink() {
                    continue;
                }

                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if !(name.ends_with(".log") || name.contains(".log.")) {
                    continue;
                }

                let Ok(metadata) = entry.metadata() else {
                    result.add_skipped(path.to_path_buf(), "Cannot read file metadata");
                    continue;
                };
                let modified = match metadata.modified() {
                    Ok(modified) => modified,
                    Err(_) => {
                        result.add_skipped(
                            path.to_path_buf(),
                            "Cannot determine file age; kept for safety",
                        );
                        continue;
                    }
                };
                if modified > cutoff {
                    continue;
                }

                let audit = self.security.audit_for_deletion(path);
                if !audit.is_safe {
                    for violation in audit.violations {
                        result.add_skipped(path.to_path_buf(), violation.to_string());
                    }
                    continue;
                }

                result.add_file(FileEntry::new(
                    path.to_path_buf(),
                    metadata.len(),
                    false,
                    false,
                ));
            }
        }

        result
    }

    pub fn scan_directory(&self, path: &Path, include_dirs: bool) -> ScanResult {
        self.scan_directory_with_options(path, include_dirs, None, 0)
    }

    fn scan_browser_pattern(&self, pattern: &Path, recurse_dirs: bool) -> ScanResult {
        let pattern_str = pattern.to_string_lossy().to_string();

        if !(pattern_str.contains('*') || pattern_str.contains('?') || pattern_str.contains('[')) {
            return self.scan_browser_target(pattern, recurse_dirs);
        }

        let mut result = ScanResult::new();

        match glob::glob(&pattern_str) {
            Ok(paths) => {
                let mut matched = false;
                for entry in paths.filter_map(|p| p.ok()) {
                    if self.is_cancelled() {
                        result.cancelled = true;
                        break;
                    }
                    matched = true;
                    result.merge(self.scan_browser_target(&entry, recurse_dirs));
                }

                if !matched {
                    result.add_skipped(pattern.to_path_buf(), "Path does not exist");
                }
            }
            Err(e) => {
                result.add_error(format!("Invalid browser glob pattern: {}", e));
            }
        }

        result
    }

    fn scan_browser_target(&self, path: &Path, recurse_dirs: bool) -> ScanResult {
        let mut result = ScanResult::new();

        let audit = self.security.audit(path);
        if !audit.is_safe {
            for violation in audit.violations {
                result.add_security_violation(violation.to_string());
            }
            return result;
        }

        if !path.exists() {
            result.add_skipped(path.to_path_buf(), "Path does not exist");
            return result;
        }

        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(e) => {
                result.add_error(format!("Cannot read metadata for {}: {}", path.display(), e));
                return result;
            }
        };

        let entry_audit = self.security.audit_for_deletion(path);
        if !entry_audit.is_safe {
            for violation in entry_audit.violations {
                result.add_skipped(path.to_path_buf(), violation.to_string());
            }
            return result;
        }

        let is_symlink = metadata.file_type().is_symlink();
        if metadata.is_dir() {
            result.merge(self.scan_directory(path, recurse_dirs));
        } else {
            result.add_file(FileEntry::new(
                path.to_path_buf(),
                metadata.len(),
                false,
                is_symlink,
            ));
        }

        result
    }

    pub fn scan_directory_with_options(
        &self,
        path: &Path,
        include_dirs: bool,
        file_pattern: Option<&str>,
        min_age_days: u32,
    ) -> ScanResult {
        let mut result = ScanResult::new();

        let audit = self.security.audit(path);
        if !audit.is_safe {
            for violation in audit.violations {
                result.add_security_violation(violation.to_string());
            }
            return result;
        }

        if !path.exists() {
            result.add_skipped(path.to_path_buf(), "Path does not exist");
            return result;
        }

        let age_threshold = if min_age_days > 0 {
            std::time::SystemTime::now()
                .checked_sub(std::time::Duration::from_secs(min_age_days as u64 * 86400))
        } else {
            None
        };

        let walker = WalkDir::new(path).follow_links(false).into_iter();

        for entry in walker.filter_map(|e| e.ok()) {
            if self.is_cancelled() {
                result.cancelled = true;
                break;
            }

            let entry_path = entry.path();

            if entry_path == path {
                continue;
            }

            let entry_audit = self.security.audit_for_deletion(entry_path);
            if !entry_audit.is_safe {
                for v in entry_audit.violations {
                    result.add_skipped(entry_path.to_path_buf(), v.to_string());
                }
                continue;
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    result.add_error(format!("Cannot read metadata for {}: {}", entry_path.display(), e));
                    continue;
                }
            };

            if let Some(pattern) = file_pattern {
                if metadata.is_file() {
                    let name = entry_path.file_name().map(|n| n.to_string_lossy());
                    if let Some(name) = name {
                        if !Self::matches_pattern(&name, pattern) {
                            continue;
                        }
                    }
                }
            }

            if let Some(threshold) = age_threshold {
                if let Ok(modified) = metadata.modified() {
                    if modified > threshold {
                        continue;
                    }
                }
            }

            let is_symlink = entry_path.is_symlink();
            let size = if metadata.is_file() { metadata.len() } else { 0 };

            let file_entry = FileEntry::new(
                entry_path.to_path_buf(),
                size,
                metadata.is_dir(),
                is_symlink,
            );

            if metadata.is_dir() {
                if include_dirs {
                    result.add_directory(file_entry);
                }
            } else {
                result.add_file(file_entry);
            }
        }

        result
    }

    fn scan_glob_pattern(&self, pattern: &Path) -> ScanResult {
        let mut result = ScanResult::new();

        let pattern_str = pattern.to_string_lossy();

        let expanded = if let Some(relative) = pattern_str.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(relative).to_string_lossy().to_string()
            } else {
                result.add_error("Cannot expand home directory");
                return result;
            }
        } else {
            pattern_str.to_string()
        };

        match glob::glob(&expanded) {
            Ok(paths) => {
                for entry in paths.filter_map(|p| p.ok()) {
                    if self.is_cancelled() {
                        result.cancelled = true;
                        break;
                    }
                    let scan = self.scan_glob_match(&entry);
                    result.merge(scan);
                }
            }
            Err(e) => {
                result.add_error(format!("Invalid glob pattern: {}", e));
            }
        }

        result
    }

    fn scan_glob_match(&self, path: &Path) -> ScanResult {
        let mut result = ScanResult::new();

        let audit = self.security.audit(path);
        if !audit.is_safe {
            for violation in audit.violations {
                result.add_security_violation(violation.to_string());
            }
            return result;
        }

        if !path.exists() {
            result.add_skipped(path.to_path_buf(), "Path does not exist");
            return result;
        }

        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(err) => {
                result.add_error(format!(
                    "Cannot read metadata for {}: {}",
                    path.display(),
                    err
                ));
                return result;
            }
        };

        let entry_audit = self.security.audit_for_deletion(path);
        if !entry_audit.is_safe {
            for violation in entry_audit.violations {
                result.add_skipped(path.to_path_buf(), violation.to_string());
            }
            return result;
        }

        if metadata.is_dir() {
            return self.scan_directory(path, false);
        }

        result.add_file(FileEntry::new(
            path.to_path_buf(),
            metadata.len(),
            false,
            metadata.file_type().is_symlink(),
        ));
        result
    }

    fn matches_pattern(name: &str, pattern: &str) -> bool {
        if let Some(middle) = pattern
            .strip_prefix('*')
            .and_then(|value| value.strip_suffix('*'))
        {
            name.contains(middle)
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            name.ends_with(suffix)
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            name.starts_with(prefix)
        } else {
            name == pattern
        }
    }

}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::fs::FileTimes;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    #[test]
    fn browser_scan_only_targets_selected_chrome_data_files() {
        let dir = TempDir::new().unwrap();
        let profile = dir.path().join("Default");
        fs::create_dir_all(profile.join("Cache/Cache_Data")).unwrap();
        fs::create_dir_all(profile.join("Code Cache/js")).unwrap();
        fs::create_dir_all(profile.join("GPUCache")).unwrap();
        fs::create_dir_all(profile.join("GrShaderCache")).unwrap();
        fs::create_dir_all(profile.join("GraphiteDawnCache")).unwrap();
        fs::write(profile.join("Cookies"), b"cookies").unwrap();
        fs::write(profile.join("History"), b"history").unwrap();
        fs::write(profile.join("Preferences"), b"prefs").unwrap();
        fs::write(profile.join("Bookmarks"), b"bookmarks").unwrap();
        fs::write(profile.join("Cache/index"), b"cache").unwrap();
        fs::write(profile.join("Code Cache/js/index"), b"code-cache").unwrap();
        fs::write(profile.join("GPUCache/data_0"), b"gpu-cache").unwrap();
        fs::write(profile.join("GrShaderCache/data_1"), b"shader-cache").unwrap();
        fs::write(
            profile.join("GraphiteDawnCache/data_2"),
            b"graphite-cache",
        )
        .unwrap();

        let mut cookies = BrowserRule::new(BrowserType::Chrome, BrowserDataType::Cookies);
        cookies.enabled = true;
        cookies.custom_path = Some(dir.path().to_path_buf());

        let mut history = BrowserRule::new(BrowserType::Chrome, BrowserDataType::History);
        history.enabled = true;
        history.custom_path = Some(dir.path().to_path_buf());

        let mut downloads = BrowserRule::new(BrowserType::Chrome, BrowserDataType::DownloadHistory);
        downloads.enabled = true;
        downloads.custom_path = Some(dir.path().to_path_buf());

        let mut cache = BrowserRule::new(BrowserType::Chrome, BrowserDataType::Cache);
        cache.enabled = true;
        cache.custom_path = Some(dir.path().to_path_buf());

        let scanner = Scanner::new();
        let result = scanner.scan_browser_rules(&[cookies, history, downloads, cache]);

        let file_paths: HashSet<PathBuf> = result.files.iter().map(|entry| entry.path.clone()).collect();
        let dir_paths: HashSet<PathBuf> = result.directories.iter().map(|entry| entry.path.clone()).collect();

        assert!(file_paths.contains(&profile.join("Cookies")));
        assert!(file_paths.contains(&profile.join("History")));
        assert!(file_paths.contains(&profile.join("Cache/index")));
        assert!(file_paths.contains(&profile.join("Code Cache/js/index")));
        assert!(file_paths.contains(&profile.join("GPUCache/data_0")));
        assert!(file_paths.contains(&profile.join("GrShaderCache/data_1")));
        assert!(file_paths.contains(&profile.join("GraphiteDawnCache/data_2")));
        assert!(!file_paths.contains(&profile.join("Preferences")));
        assert!(!file_paths.contains(&profile.join("Bookmarks")));
        assert_eq!(
            file_paths.iter().filter(|path| **path == profile.join("History")).count(),
            1
        );
        assert!(dir_paths.contains(&profile.join("Cache/Cache_Data")));
        assert!(dir_paths.contains(&profile.join("Code Cache/js")));
        assert!(!dir_paths.contains(&profile));
    }

    #[test]
    fn browser_scan_supports_website_storage_and_crash_reports() {
        let dir = TempDir::new().unwrap();
        let profile = dir.path().join("Default");
        fs::create_dir_all(profile.join("Local Storage/leveldb")).unwrap();
        fs::create_dir_all(dir.path().join("Crash Reports/completed")).unwrap();
        fs::write(profile.join("Local Storage/leveldb/data"), b"site-data").unwrap();
        fs::write(
            dir.path().join("Crash Reports/completed/report.dmp"),
            b"crash",
        )
        .unwrap();
        fs::write(profile.join("Bookmarks"), b"keep").unwrap();

        let mut site_data =
            BrowserRule::new(BrowserType::Yandex, BrowserDataType::SiteData);
        site_data.enabled = true;
        site_data.custom_path = Some(dir.path().to_path_buf());

        let mut crash_reports =
            BrowserRule::new(BrowserType::Yandex, BrowserDataType::CrashReports);
        crash_reports.enabled = true;
        crash_reports.custom_path = Some(dir.path().to_path_buf());

        let result = Scanner::new().scan_browser_rules(&[site_data, crash_reports]);
        let file_paths: HashSet<PathBuf> =
            result.files.iter().map(|entry| entry.path.clone()).collect();

        assert!(file_paths.contains(&profile.join("Local Storage/leveldb/data")));
        assert!(file_paths.contains(&dir.path().join("Crash Reports/completed/report.dmp")));
        assert!(!file_paths.contains(&profile.join("Bookmarks")));
    }

    #[test]
    fn app_glob_rules_can_target_files() {
        let dir = TempDir::new().unwrap();
        let state_dir = dir.path().join("state");
        fs::create_dir_all(state_dir.join("app")).unwrap();
        fs::write(state_dir.join("root.log"), b"root-log").unwrap();
        fs::write(state_dir.join("root.txt"), b"root-text").unwrap();
        fs::write(state_dir.join("app/app.log"), b"app-log").unwrap();

        let mut top_level_logs =
            AppRule::new("User State Logs", "XDG state logs", state_dir.join("*.log"));
        top_level_logs.enabled = true;

        let mut nested_logs =
            AppRule::new(
                "Application State Logs",
                "Per-application XDG state logs",
                state_dir.join("*/*.log"),
            );
        nested_logs.enabled = true;

        let scanner = Scanner::new();
        let result = scanner.scan_app_rules(&[top_level_logs, nested_logs]);
        let file_paths: HashSet<PathBuf> =
            result.files.iter().map(|entry| entry.path.clone()).collect();

        assert!(file_paths.contains(&state_dir.join("root.log")));
        assert!(file_paths.contains(&state_dir.join("app/app.log")));
        assert!(!file_paths.contains(&state_dir.join("root.txt")));
    }

    #[test]
    fn firefox_history_is_skipped_as_unsafe() {
        let dir = TempDir::new().unwrap();
        let profile = dir.path().join("abcd.default-release");
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("places.sqlite"), b"places").unwrap();

        let mut history = BrowserRule::new(BrowserType::Firefox, BrowserDataType::History);
        history.enabled = true;
        history.custom_path = Some(dir.path().to_path_buf());

        let scanner = Scanner::new();
        let result = scanner.scan_browser_rules(&[history]);

        assert!(result.files.is_empty());
        assert!(result.directories.is_empty());
        assert_eq!(result.skipped.len(), 1);
    }

    #[test]
    fn cleanup_scan_never_follows_directory_symlinks() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("root");
        let target = dir.path().join("target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep.log"), b"log").unwrap();

        let link = root.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let without_links = Scanner::new().scan_directory(&root, false);
        let without_paths: HashSet<PathBuf> = without_links
            .files
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        assert!(!without_paths.contains(&link.join("keep.log")));

        let with_links = Scanner::with_options(ScanOptions {
            ..Default::default()
        })
        .scan_directory(&root, false);
        let with_paths: HashSet<PathBuf> = with_links
            .files
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        assert!(!with_paths.contains(&link.join("keep.log")));

        let cleaned = crate::services::Cleaner::new().clean(&with_links);
        assert!(cleaned.deleted_files.is_empty());
        assert!(target.join("keep.log").exists());
    }

    #[test]
    fn external_cancellation_marks_scan_result() {
        let dir = TempDir::new().unwrap();
        let cancellation = Arc::new(AtomicBool::new(true));
        let scanner = Scanner::with_options_and_cancellation(
            ScanOptions::default(),
            cancellation,
        );

        let result = scanner.scan_directory(dir.path(), false);

        assert!(result.cancelled);
    }

    #[test]
    fn application_log_scan_only_includes_old_log_files() {
        let dir = TempDir::new().unwrap();
        let app_dir = dir.path().join("example-app");
        fs::create_dir_all(&app_dir).unwrap();

        let old_log = app_dir.join("application.log");
        let rotated_log = app_dir.join("application.log.1");
        let recent_log = app_dir.join("recent.log");
        let unrelated_file = app_dir.join("keep.txt");
        fs::write(&old_log, b"old log").unwrap();
        fs::write(&rotated_log, b"rotated log").unwrap();
        fs::write(&recent_log, b"recent log").unwrap();
        fs::write(&unrelated_file, b"not a log").unwrap();

        let old_time = SystemTime::now() - Duration::from_secs(31 * 86_400);
        let old_times = FileTimes::new().set_modified(old_time);
        fs::File::options()
            .write(true)
            .open(&old_log)
            .unwrap()
            .set_times(old_times)
            .unwrap();
        fs::File::options()
            .write(true)
            .open(&rotated_log)
            .unwrap()
            .set_times(old_times)
            .unwrap();

        let result = Scanner::new().scan_application_log_paths(
            &[dir.path().to_path_buf()],
            30,
        );
        let paths: HashSet<_> = result.files.iter().map(|entry| &entry.path).collect();

        assert!(paths.contains(&old_log));
        assert!(paths.contains(&rotated_log));
        assert!(!paths.contains(&recent_log));
        assert!(!paths.contains(&unrelated_file));
    }

    #[test]
    fn application_log_scan_never_follows_symlinks() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("logs");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let outside_log = outside.join("outside.log");
        fs::write(&outside_log, b"outside log").unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(31 * 86_400);
        fs::File::options()
            .write(true)
            .open(&outside_log)
            .unwrap()
            .set_times(FileTimes::new().set_modified(old_time))
            .unwrap();
        std::os::unix::fs::symlink(&outside, root.join("linked-logs")).unwrap();

        let scanner = Scanner::with_options(ScanOptions {
            log_retention_days: 30,
        });
        let result = scanner.scan_application_log_paths(&[root], 30);

        assert!(result.files.is_empty());
    }

    #[test]
    fn zz_verifier_overlapping_system_rules_duplicate_entries() {
        use crate::models::{SystemRule, SystemRuleType};
        let dir = TempDir::new().unwrap();
        let cache = dir.path().join("cache");
        let thumbs = cache.join("thumbnails/normal");
        fs::create_dir_all(&thumbs).unwrap();
        fs::write(thumbs.join("a.png"), vec![0u8; 1000]).unwrap();
        fs::write(thumbs.join("b.png"), vec![0u8; 1000]).unwrap();

        let mut user_cache = SystemRule::new(SystemRuleType::UserCache);
        user_cache.enabled = true;
        user_cache.custom_paths = Some(vec![cache.clone()]);

        let mut thumbnails = SystemRule::new(SystemRuleType::Thumbnails);
        thumbnails.enabled = true;
        thumbnails.custom_paths = Some(vec![cache.join("thumbnails")]);

        let scanner = Scanner::new();
        let result = scanner.scan_system_rules(&[user_cache, thumbnails]);
        let a = thumbs.join("a.png");
        let dupes = result.files.iter().filter(|f| f.path == a).count();
        eprintln!("VERIFIER: file_count={} total_size={} dupes_of_a={}",
            result.file_count, result.total_size, dupes);

        // now clean and see what happens
        let cleaner = crate::services::Cleaner::new();
        let clean = cleaner.clean(&result);
        eprintln!("VERIFIER: deleted={} freed={} failed={:?}",
            clean.deleted_files.len(), clean.bytes_freed, clean.failed);
        assert_eq!(dupes, 1, "expected dedup but got {} copies", dupes);
    }
}
