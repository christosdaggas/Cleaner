use crate::models::{BrowserType, CleanError, CleanResult, FileEntry, ScanResult};
use crate::services::SecurityAuditor;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Copy)]
pub struct CleanOptions {
    pub max_files_per_operation: usize,
    pub max_size_per_operation: u64,
}

impl Default for CleanOptions {
    fn default() -> Self {
        Self {
            max_files_per_operation: usize::MAX,
            max_size_per_operation: u64::MAX,
        }
    }
}

pub struct Cleaner {
    security: SecurityAuditor,
    cancelled: Arc<AtomicBool>,
    options: CleanOptions,
}

impl Cleaner {
    pub fn new() -> Self {
        Self::with_options(CleanOptions::default())
    }

    pub fn with_options(options: CleanOptions) -> Self {
        Self::with_options_and_cancellation(options, Arc::new(AtomicBool::new(false)))
    }

    pub fn with_options_and_cancellation(
        options: CleanOptions,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        // Directory symlinks are deliberately never traversed. Keeping this
        // field false also protects users with an older config that enabled
        // the removed unsafe option.
        let security = SecurityAuditor::new();

        Self {
            security,
            cancelled,
            options,
        }
    }

    #[cfg(test)]
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn validate_operation(&self, scan_result: &ScanResult) -> Result<(), String> {
        if scan_result.file_count > self.options.max_files_per_operation {
            return Err(format!(
                "Cleanup blocked by safety limit: {} files found, limit is {}.",
                scan_result.file_count, self.options.max_files_per_operation
            ));
        }

        if scan_result.total_size > self.options.max_size_per_operation {
            return Err(format!(
                "Cleanup blocked by safety limit: {} selected, limit is {}.",
                scan_result.formatted_size(),
                bytesize::ByteSize(self.options.max_size_per_operation)
            ));
        }

        Ok(())
    }

    #[cfg(test)]
    pub fn dry_run(&self, scan_result: &ScanResult) -> CleanResult {
        let mut result = CleanResult::new();

        if let Err(reason) = self.validate_operation(scan_result) {
            result.blocked(reason);
            return result;
        }

        for file in &scan_result.files {
            if self.is_cancelled() {
                result.cancelled = true;
                break;
            }

            let audit = self.security.audit_for_deletion(&file.path);
            if audit.is_safe {
                result.add_deleted_file(file.path.clone(), file.size);
            } else {
                for v in audit.violations {
                    result.add_skipped(file.path.clone(), v.to_string());
                }
            }
        }

        for dir in &scan_result.directories {
            if self.is_cancelled() {
                result.cancelled = true;
                break;
            }

            let audit = self.security.audit_for_deletion(&dir.path);
            if audit.is_safe {
                result.add_deleted_directory(dir.path.clone());
            } else {
                for v in audit.violations {
                    result.add_skipped(dir.path.clone(), v.to_string());
                }
            }
        }

        result
    }

    /// Build the (browser, base-path) lookup table once. Each entry's path
    /// comes from `cache_dir()`/`home_dir()` which both perform syscalls,
    /// so this is intentionally computed once per cleanup rather than per
    /// file.
    fn browser_path_table() -> Vec<(BrowserType, PathBuf)> {
        let mut paths = Vec::with_capacity(BrowserType::all().len() * 2);
        for browser in BrowserType::all() {
            if let Some(p) = browser.cache_path() {
                paths.push((*browser, p));
            }
            if let Some(p) = browser.config_path() {
                paths.push((*browser, p));
            }
        }
        paths
    }

    /// Check if a path belongs to a browser directory using a precomputed
    /// (browser, base-path) lookup table.
    fn browser_for_path(table: &[(BrowserType, PathBuf)], path: &Path) -> Option<BrowserType> {
        table
            .iter()
            .find(|(_, base)| path.starts_with(base))
            .map(|(browser, _)| *browser)
    }

    pub fn clean(&self, scan_result: &ScanResult) -> CleanResult {
        let mut result = CleanResult::new();

        if let Err(reason) = self.validate_operation(scan_result) {
            result.blocked(reason);
            warn!("{}", result.blocked_reason.as_deref().unwrap_or("Cleanup blocked"));
            return result;
        }

        if self.is_cancelled() {
            result.cancelled = true;
            return result;
        }

        info!(
            "Starting cleanup: {} files, {} directories, {} total",
            scan_result.file_count,
            scan_result.dir_count,
            scan_result.formatted_size()
        );

        // Precompute browser detection state once instead of rescanning /proc
        // and rebuilding base paths for every file. Without this, a cleanup of
        // N files runs N * 9 directory walks of /proc, which freezes the UI
        // for large scan results.
        let browser_table = Self::browser_path_table();
        let running_browsers: HashSet<BrowserType> = BrowserType::running_browsers();

        for file in &scan_result.files {
            if self.is_cancelled() {
                result.cancelled = true;
                warn!("Cleanup cancelled by user");
                break;
            }

            // Re-check if the browser is running to prevent SQLite corruption (TOCTOU mitigation)
            if let Some(browser) = Self::browser_for_path(&browser_table, &file.path) {
                if running_browsers.contains(&browser) {
                    result.add_skipped(
                        file.path.clone(),
                        format!(
                            "{} is currently running — close it before cleaning to avoid data corruption",
                            browser.display_name()
                        ),
                    );
                    continue;
                }
            }

            match self.delete_file(&file.path, file.size) {
                Ok(size) => {
                    result.add_deleted_file(file.path.clone(), size);
                    debug!("Deleted file: {}", file.path.display());
                }
                Err(e) => {
                    result.add_failed(file.path.clone(), e.to_string());
                    warn!("Failed to delete {}: {}", file.path.display(), e);
                }
            }
        }

        let mut dirs: Vec<&FileEntry> = scan_result.directories.iter().collect();
        dirs.sort_by_key(|entry| std::cmp::Reverse(entry.path.components().count()));

        for dir in dirs {
            if self.is_cancelled() {
                result.cancelled = true;
                break;
            }

            match self.delete_directory(&dir.path) {
                Ok(()) => {
                    result.add_deleted_directory(dir.path.clone());
                    debug!("Deleted directory: {}", dir.path.display());
                }
                Err(e) => {
                    if matches!(e, CleanError::DirectoryNotEmpty(_)) {
                        result.add_skipped(
                            dir.path.clone(),
                            "Directory was preserved because it is not empty",
                        );
                    } else {
                        result.add_failed(dir.path.clone(), e.to_string());
                        warn!("Failed to delete directory {}: {}", dir.path.display(), e);
                    }
                }
            }
        }

        info!(
            "Cleanup complete: {} files deleted, {} freed, {} failed",
            result.files_deleted_count(),
            result.formatted_bytes_freed(),
            result.failed_count()
        );

        result
    }

    fn delete_file(&self, path: &Path, _expected_size: u64) -> Result<u64, CleanError> {
        let audit = self.security.audit_for_deletion(path);
        if !audit.is_safe {
            let violation = audit.violations.into_iter().next()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "Unknown security violation".to_string());
            return Err(CleanError::SecurityViolation(violation));
        }

        let metadata = self.security.verify_before_deletion(path)
            .map_err(|e| CleanError::SecurityViolation(e.to_string()))?;

        let actual_size = metadata.len();

        fs::remove_file(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                CleanError::PermissionDenied(path.to_path_buf())
            } else {
                CleanError::IoError(e)
            }
        })?;

        Ok(actual_size)
    }

    fn delete_directory(&self, path: &Path) -> Result<(), CleanError> {
        let audit = self.security.audit_for_deletion(path);
        if !audit.is_safe {
            let violation = audit.violations.into_iter().next()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "Unknown security violation".to_string());
            return Err(CleanError::SecurityViolation(violation));
        }

        self.security.verify_before_deletion(path)
            .map_err(|e| CleanError::SecurityViolation(e.to_string()))?;

        let is_empty = path.read_dir()
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);

        if !is_empty {
            return Err(CleanError::DirectoryNotEmpty(path.to_path_buf()));
        }

        fs::remove_dir(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                CleanError::PermissionDenied(path.to_path_buf())
            } else {
                CleanError::IoError(e)
            }
        })?;

        Ok(())
    }
}

impl Default for Cleaner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();

        let file1 = dir.path().join("test1.txt");
        let mut f = File::create(&file1).unwrap();
        f.write_all(b"test content 1").unwrap();

        let file2 = dir.path().join("test2.log");
        let mut f = File::create(&file2).unwrap();
        f.write_all(b"test content 2 longer").unwrap();

        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        let file3 = subdir.join("nested.txt");
        let mut f = File::create(&file3).unwrap();
        f.write_all(b"nested content").unwrap();

        dir
    }

    #[test]
    fn test_dry_run_no_deletion() {
        let dir = setup_test_dir();
        let cleaner = Cleaner::new();

        let mut scan = ScanResult::new();
        let file = dir.path().join("test1.txt");
        scan.add_file(FileEntry::new(file.clone(), 14, false, false));

        let result = cleaner.dry_run(&scan);

        assert!(file.exists());
        assert_eq!(result.files_deleted_count(), 1);
    }

    #[test]
    fn test_clean_deletes_files() {
        let dir = setup_test_dir();
        let cleaner = Cleaner::new();

        let mut scan = ScanResult::new();
        let file = dir.path().join("test1.txt");
        scan.add_file(FileEntry::new(file.clone(), 14, false, false));

        let result = cleaner.clean(&scan);
        assert_eq!(result.files_deleted_count(), 1);
        assert!(!file.exists());
    }

    #[test]
    fn test_cancellation() {
        let cleaner = Cleaner::new();
        cleaner.cancel();

        let scan = ScanResult::new();
        let result = cleaner.clean(&scan);

        assert!(result.cancelled);
    }

    #[test]
    fn cleanup_is_blocked_when_file_limit_is_exceeded() {
        let dir = setup_test_dir();
        let cleaner = Cleaner::with_options(CleanOptions {
            max_files_per_operation: 1,
            ..CleanOptions::default()
        });

        let mut scan = ScanResult::new();
        scan.add_file(FileEntry::new(dir.path().join("test1.txt"), 14, false, false));
        scan.add_file(FileEntry::new(dir.path().join("test2.log"), 21, false, false));

        let result = cleaner.clean(&scan);
        assert!(result.is_blocked());
        assert!(dir.path().join("test1.txt").exists());
    }
}
