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

                if rule.is_subfolder_search() {
                    let name = rule
                        .subfolder_name
                        .as_deref()
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    result.merge(self.scan_subfolder_search(
                        &path,
                        &name,
                        include_dirs,
                        rule.file_pattern.as_deref(),
                        rule.min_age_days,
                    ));
                } else {
                    result.merge(self.scan_directory_with_options(
                        &path,
                        include_dirs,
                        rule.file_pattern.as_deref(),
                        rule.min_age_days,
                    ));
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

    /// Scan a rule that searches `root` for subdirectories whose path
    /// matches the pattern `name` (a single folder name such as `"target"`
    /// or a relative path such as `"target/debug"`) and treats every match
    /// as a cleanup target. With
    /// `DeletionMode::FilesAndDirectories` each matched folder itself is
    /// deleted (contents first); with `FilesOnly` only the files inside
    /// matches are removed and the folder structure is kept.
    fn scan_subfolder_search(
        &self,
        root: &Path,
        name: &str,
        include_dirs: bool,
        file_pattern: Option<&str>,
        min_age_days: u32,
    ) -> ScanResult {
        let mut result = ScanResult::new();

        let root_audit = self.security.audit(root);
        if !root_audit.is_safe {
            for violation in root_audit.violations {
                result.add_security_violation(violation.to_string());
            }
            return result;
        }

        if !root.exists() {
            result.add_skipped(root.to_path_buf(), "Path does not exist");
            return result;
        }

        let Some(segments) = Self::parse_subfolder_pattern(name) else {
            result.add_error(format!("Invalid subfolder search pattern '{}'", name));
            return result;
        };

        let matches = self.find_subdirectories_with_segments(root, &segments);
        if matches.is_empty() {
            result.add_skipped(
                root.to_path_buf(),
                format!("No subdirectories matching '{}' found", name),
            );
            return result;
        }

        let match_results: Vec<ScanResult> = matches
            .par_iter()
            .map(|match_path| self.scan_subfolder_match(match_path, include_dirs, file_pattern, min_age_days))
            .collect();

        for r in match_results {
            result.merge(r);
        }
        result.cancelled |= self.is_cancelled();
        result
    }

    fn scan_subfolder_match(
        &self,
        match_path: &Path,
        include_dirs: bool,
        file_pattern: Option<&str>,
        min_age_days: u32,
    ) -> ScanResult {
        let mut match_result = ScanResult::new();

        let audit = self.security.audit(match_path);
        if !audit.is_safe {
            for violation in audit.violations {
                match_result.add_skipped(match_path.to_path_buf(), violation.to_string());
            }
            return match_result;
        }

        match_result.merge(self.scan_directory_with_options(
            match_path,
            include_dirs,
            file_pattern,
            min_age_days,
        ));

        // In FilesAndDirectories mode the matched folder itself is removed
        // once its contents are gone. The cleaner deletes directories
        // deepest-first, so adding the folder here is enough for the whole
        // tree to disappear.
        if include_dirs && !self.is_cancelled() {
            let entry_audit = self.security.audit_for_deletion(match_path);
            if entry_audit.is_safe {
                let is_symlink = std::fs::symlink_metadata(match_path)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);
                match_result.add_directory(FileEntry::new(
                    match_path.to_path_buf(),
                    0,
                    true,
                    is_symlink,
                ));
            } else {
                for v in entry_audit.violations {
                    match_result.add_skipped(match_path.to_path_buf(), v.to_string());
                }
            }
        }

        match_result
    }

    /// Parse a subfolder search pattern such as `"target"` or
    /// `"target/debug"` into normalized path segments. Segments are
    /// trimmed and empty ones (from stray slashes or whitespace) are
    /// dropped. Returns `None` for patterns without a usable segment or
    /// that try to traverse (`.` or `..`).
    pub fn parse_subfolder_pattern(pattern: &str) -> Option<Vec<String>> {
        let segments: Vec<String> = pattern
            .split('/')
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect();

        if segments.is_empty() || segments.iter().any(|segment| segment == "." || segment == "..")
        {
            return None;
        }

        Some(segments)
    }

    /// Find directories whose path below `root` ends with the given
    /// pattern, e.g. pattern `"target/debug"` matches
    /// `root/project-a/target/debug`. The root itself is never considered
    /// a match. Directory symlinks are neither followed nor matched, so
    /// the search cannot escape the tree. Once a directory matches, its
    /// subtree is not descended into again: an outer match already covers
    /// any same-named folders inside it.
    pub fn find_matching_subdirectories(&self, root: &Path, pattern: &str) -> Vec<PathBuf> {
        let Some(segments) = Self::parse_subfolder_pattern(pattern) else {
            return Vec::new();
        };
        self.find_subdirectories_with_segments(root, &segments)
    }

    fn find_subdirectories_with_segments(&self, root: &Path, segments: &[String]) -> Vec<PathBuf> {
        let mut matches = Vec::new();

        let mut walker = WalkDir::new(root)
            .follow_links(false)
            .min_depth(segments.len())
            .into_iter();

        while let Some(entry) = walker.next() {
            if self.is_cancelled() {
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            if !entry.file_type().is_dir() {
                continue;
            }
            if !Self::directory_matches_pattern(entry.path(), root, segments) {
                continue;
            }
            matches.push(entry.path().to_path_buf());
            walker.skip_current_dir();
        }

        matches
    }

    fn directory_matches_pattern(path: &Path, root: &Path, segments: &[String]) -> bool {
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };

        let components: Vec<&std::ffi::OsStr> = relative
            .components()
            .map(|component| component.as_os_str())
            .collect();

        if components.len() < segments.len() {
            return false;
        }

        components[components.len() - segments.len()..]
            .iter()
            .zip(segments.iter())
            .all(|(component, segment)| *component == std::ffi::OsStr::new(segment.as_str()))
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
    fn subfolder_search_finds_targets_and_covers_their_contents() {
        let dir = TempDir::new().unwrap();
        let projects = dir.path().join("Developer");
        let alpha_target = projects.join("alpha/target");
        let beta_target = projects.join("nested/beta/target");
        fs::create_dir_all(alpha_target.join("debug/deps")).unwrap();
        fs::create_dir_all(&beta_target).unwrap();
        fs::write(alpha_target.join("debug/deps/app.rlib"), b"rlib").unwrap();
        fs::write(alpha_target.join("binary"), b"bin").unwrap();
        fs::write(beta_target.join("artifact.o"), b"obj").unwrap();
        fs::create_dir_all(projects.join("docs")).unwrap();
        fs::write(projects.join("docs/readme.md"), b"keep").unwrap();

        let mut rule = CustomRule::new("Build folders", projects.clone());
        rule.enabled = true;
        rule.deletion_mode = DeletionMode::FilesAndDirectories;
        rule.subfolder_name = Some("target".to_string());

        let result = Scanner::new().scan_custom_rules(&[rule]);

        let dir_paths: HashSet<PathBuf> = result
            .directories
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        let file_paths: HashSet<PathBuf> =
            result.files.iter().map(|entry| entry.path.clone()).collect();

        assert!(dir_paths.contains(&alpha_target));
        assert!(dir_paths.contains(&beta_target));
        assert!(dir_paths.contains(&alpha_target.join("debug")));
        assert!(file_paths.contains(&alpha_target.join("debug/deps/app.rlib")));
        assert!(file_paths.contains(&beta_target.join("artifact.o")));

        assert!(!dir_paths.contains(&projects.join("docs")));
        assert!(!file_paths.contains(&projects.join("docs/readme.md")));
    }

    #[test]
    fn subfolder_search_files_only_mode_never_lists_directories() {
        let dir = TempDir::new().unwrap();
        let projects = dir.path().join("Developer");
        let target = projects.join("project/target");
        fs::create_dir_all(target.join("debug")).unwrap();
        fs::write(target.join("debug/app.rlib"), b"rlib").unwrap();

        let mut rule = CustomRule::new("Build folders", projects);
        rule.enabled = true;
        rule.deletion_mode = DeletionMode::FilesOnly;
        rule.subfolder_name = Some("target".to_string());

        let result = Scanner::new().scan_custom_rules(&[rule]);

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, target.join("debug/app.rlib"));
        assert!(result.directories.is_empty());
    }

    #[test]
    fn subfolder_search_reports_when_nothing_matches() {
        let dir = TempDir::new().unwrap();
        let projects = dir.path().join("Developer");
        fs::create_dir_all(projects.join("project/src")).unwrap();

        let mut rule = CustomRule::new("Build folders", projects.clone());
        rule.enabled = true;
        rule.deletion_mode = DeletionMode::FilesAndDirectories;
        rule.subfolder_name = Some("target".to_string());

        let result = Scanner::new().scan_custom_rules(&[rule]);

        assert!(result.files.is_empty());
        assert!(result.directories.is_empty());
        assert!(result.skipped.iter().any(|(path, reason)| reason
            .contains("No subdirectories matching 'target'")
            && path.ends_with("Developer")));
    }

    #[test]
    fn subfolder_search_supports_nested_patterns_like_target_debug() {
        let dir = TempDir::new().unwrap();
        let projects = dir.path().join("Developer");

        let p1_debug = projects.join("p1/target/debug");
        fs::create_dir_all(p1_debug.join("deps")).unwrap();
        fs::write(p1_debug.join("deps/app.rlib"), b"rlib").unwrap();
        fs::create_dir_all(projects.join("p1/target/release")).unwrap();
        fs::write(projects.join("p1/target/release/app.bin"), b"keep").unwrap();

        let p2_debug = projects.join("p2/target/debug");
        fs::create_dir_all(&p2_debug).unwrap();
        fs::write(p2_debug.join("artifact.o"), b"obj").unwrap();

        let bare_debug = projects.join("p3/debug");
        fs::create_dir_all(&bare_debug).unwrap();
        fs::write(bare_debug.join("keep.txt"), b"keep").unwrap();

        let mut rule = CustomRule::new("Debug build folders", projects.clone());
        rule.enabled = true;
        rule.deletion_mode = DeletionMode::FilesAndDirectories;
        rule.subfolder_name = Some("target/debug".to_string());

        let result = Scanner::new().scan_custom_rules(&[rule]);

        let dir_paths: HashSet<PathBuf> = result
            .directories
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        let file_paths: HashSet<PathBuf> =
            result.files.iter().map(|entry| entry.path.clone()).collect();

        assert!(dir_paths.contains(&p1_debug));
        assert!(dir_paths.contains(&p2_debug));
        assert!(file_paths.contains(&p1_debug.join("deps/app.rlib")));
        assert!(file_paths.contains(&p2_debug.join("artifact.o")));

        assert!(!dir_paths.contains(&projects.join("p1/target")));
        assert!(!dir_paths.contains(&projects.join("p1/target/release")));
        assert!(!file_paths.contains(&projects.join("p1/target/release/app.bin")));
        assert!(!dir_paths.contains(&bare_debug));
        assert!(!file_paths.contains(&bare_debug.join("keep.txt")));
    }

    #[test]
    fn subfolder_search_rejects_traversal_patterns() {
        let dir = TempDir::new().unwrap();
        let projects = dir.path().join("Developer");
        fs::create_dir_all(projects.join("project/src")).unwrap();

        let mut rule = CustomRule::new("Escape attempt", projects);
        rule.enabled = true;
        rule.deletion_mode = DeletionMode::FilesAndDirectories;
        rule.subfolder_name = Some("../escape".to_string());

        let result = Scanner::new().scan_custom_rules(&[rule]);

        assert!(result.files.is_empty());
        assert!(result.directories.is_empty());
        assert!(result
            .errors
            .iter()
            .any(|error| error.contains("Invalid subfolder search pattern")));
    }

    #[test]
    fn find_matching_subdirectories_prunes_nesting_and_ignores_symlinks() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("projects");
        let outer = root.join("one/target");
        let inner = outer.join("deps/target");
        fs::create_dir_all(&inner).unwrap();
        fs::create_dir_all(root.join("two/target")).unwrap();
        fs::create_dir_all(root.join("three")).unwrap();

        let outside = dir.path().join("outside");
        fs::create_dir_all(outside.join("target")).unwrap();
        std::os::unix::fs::symlink(outside.join("target"), root.join("three/target")).unwrap();

        let scanner = Scanner::new();

        let matches = scanner.find_matching_subdirectories(&root, "target");

        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&outer));
        assert!(matches.contains(&root.join("two/target")));
        assert!(!matches.iter().any(|path| path.starts_with(&inner)));
        assert!(!matches.contains(&root.join("three/target")));

        // Nested relative-path patterns only match full suffix chains.
        let chain = root.join("four/build/target");
        let chain_debug = chain.join("debug");
        fs::create_dir_all(&chain_debug).unwrap();
        fs::create_dir_all(root.join("five/build")).unwrap();
        fs::create_dir_all(root.join("six/other/build")).unwrap();

        let chain_matches = scanner.find_matching_subdirectories(&root, "build/target");
        assert_eq!(chain_matches, vec![chain.clone()]);
        assert_eq!(
            scanner.find_matching_subdirectories(&root, "target/debug"),
            vec![chain_debug]
        );

        // Invalid patterns never match anything.
        assert!(scanner
            .find_matching_subdirectories(&root, "..")
            .is_empty());
        assert!(scanner.find_matching_subdirectories(&root, "  ").is_empty());
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
