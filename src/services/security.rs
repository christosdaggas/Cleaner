use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("Path is in hard denylist: {0}")]
    HardDenylist(PathBuf),

    #[error("Path resolves to protected location: {0}")]
    ProtectedPath(PathBuf),

    #[error("Relative path not allowed: {0}")]
    RelativePath(PathBuf),

    #[error("Empty path not allowed")]
    EmptyPath,

    #[error("Symlink escape detected: {original} resolves to {resolved}")]
    SymlinkEscape { original: PathBuf, resolved: PathBuf },

    #[error("Path depth too shallow (depth {depth}, minimum {min}): {path}")]
    PathTooShallow { path: PathBuf, depth: usize, min: usize },

    #[error("Path not owned by current user: {0}")]
    NotOwned(PathBuf),

    #[error("Path is a mount point: {0}")]
    MountPoint(PathBuf),

    #[error("Path traversal detected: {0}")]
    PathTraversal(PathBuf),

    #[error("Failed to canonicalize path: {0}")]
    CanonicalizationFailed(PathBuf),
}

#[derive(Debug)]
pub struct AuditResult {
    pub canonical_path: PathBuf,
    pub is_safe: bool,
    pub violations: Vec<SecurityError>,
    pub warnings: Vec<String>,
}

impl AuditResult {
    pub fn safe(canonical_path: PathBuf) -> Self {
        Self {
            canonical_path,
            is_safe: true,
            violations: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn unsafe_path(path: PathBuf, error: SecurityError) -> Self {
        Self {
            canonical_path: path.clone(),
            is_safe: false,
            violations: vec![error],
            warnings: Vec::new(),
        }
    }
}

pub struct SecurityAuditor {
    hard_denylist: HashSet<PathBuf>,
    prefix_denylist: Vec<PathBuf>,
    allowed_user_bases: Vec<PathBuf>,
    protected_home_paths: HashSet<PathBuf>,
    protected_cache_paths: HashSet<PathBuf>,
    current_uid: u32,
    min_path_depth: usize,
}

impl SecurityAuditor {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/nonexistent"));
        let cache = dirs::cache_dir().unwrap_or_else(|| home.join(".cache"));
        let data_local = dirs::data_local_dir().unwrap_or_else(|| home.join(".local/share"));
        let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
        let state = dirs::state_dir().unwrap_or_else(|| home.join(".local/state"));

        let mut auditor = Self {
            hard_denylist: HashSet::new(),
            prefix_denylist: Vec::new(),
            allowed_user_bases: Vec::new(),
            protected_home_paths: HashSet::new(),
            protected_cache_paths: HashSet::new(),
            current_uid: unsafe { libc::getuid() },
            min_path_depth: 3,
        };

        // Hard denylist
        let hard_deny = [
            "/", "/bin", "/sbin", "/usr", "/usr/bin", "/usr/sbin", "/usr/lib", "/usr/lib64",
            "/usr/lib32", "/usr/libexec", "/usr/share", "/usr/local", "/lib", "/lib64",
            "/lib32", "/etc", "/boot", "/dev", "/proc", "/sys", "/run", "/var", "/var/lib",
            "/var/log", "/var/run", "/root", "/home", "/opt", "/srv", "/mnt", "/media",
        ];
        for path in hard_deny {
            auditor.hard_denylist.insert(PathBuf::from(path));
        }
        auditor.hard_denylist.insert(home.clone());

        // Prefix denylist
        auditor.prefix_denylist = vec![
            PathBuf::from("/usr"), PathBuf::from("/lib"), PathBuf::from("/bin"),
            PathBuf::from("/sbin"), PathBuf::from("/etc"), PathBuf::from("/boot"),
            PathBuf::from("/dev"), PathBuf::from("/proc"), PathBuf::from("/sys"),
            PathBuf::from("/run"), PathBuf::from("/var"), PathBuf::from("/root"),
            PathBuf::from("/opt"), PathBuf::from("/srv"), PathBuf::from("/mnt"),
            PathBuf::from("/media"),
        ];

        // Allowed user bases
        auditor.allowed_user_bases = vec![
            cache.clone(),
            data_local.clone(),
            config.clone(),
            home.join(".mozilla"),
            home.join(".local"),
            home.join(".var"),
            state,
            PathBuf::from("/tmp"),
        ];

        // Protected home paths
        let protected = [
            ".ssh", ".gnupg", ".gpg", ".pki", ".password-store", ".local/share/keyrings",
            "Documents", "Pictures", "Videos", "Music", "Downloads", "Desktop", "Templates",
            "Public", ".bashrc", ".bash_profile", ".profile", ".zshrc", ".config/autostart",
            ".config/systemd",
        ];
        for path in protected {
            auditor.protected_home_paths.insert(home.join(path));
        }

        // Protected cache directories
        let protected_caches = [
            "gtk-4.0", "gtk-3.0", "icon-cache", "icons", "gnome-shell", "gnome-software",
            "gnome-settings-daemon", "fontconfig", "mesa_shader_cache", "radv_builtin_shaders",
            "gstreamer-1.0", "evolution", "ibus", "session", "tracker3", "folks", "samba",
        ];
        for cache_name in protected_caches {
            auditor.protected_cache_paths.insert(cache.join(cache_name));
        }

        // Protected local share directories
        let protected_local_share = [
            "icons", "gnome-shell", "gnome-settings-daemon", "gnome-software", "keyrings",
            "fonts", "mime", "applications", "gvfs-metadata", "recently-used.xbel",
        ];
        for dir_name in protected_local_share {
            auditor.protected_cache_paths.insert(data_local.join(dir_name));
        }

        auditor
    }

    pub fn audit(&self, path: &Path) -> AuditResult {
        if path.as_os_str().is_empty() {
            return AuditResult::unsafe_path(path.to_path_buf(), SecurityError::EmptyPath);
        }

        if !path.is_absolute() {
            return AuditResult::unsafe_path(
                path.to_path_buf(),
                SecurityError::RelativePath(path.to_path_buf()),
            );
        }

        let path_str = path.to_string_lossy();
        if path_str.contains("..") {
            return AuditResult::unsafe_path(
                path.to_path_buf(),
                SecurityError::PathTraversal(path.to_path_buf()),
            );
        }

        if self.hard_denylist.contains(path) {
            return AuditResult::unsafe_path(
                path.to_path_buf(),
                SecurityError::HardDenylist(path.to_path_buf()),
            );
        }

        for prefix in &self.prefix_denylist {
            if path.starts_with(prefix)
                && path != prefix
                && !self.is_in_allowed_base(path)
            {
                return AuditResult::unsafe_path(
                    path.to_path_buf(),
                    SecurityError::ProtectedPath(path.to_path_buf()),
                );
            }
        }

        let depth = path.components().count();
        if depth < self.min_path_depth {
            return AuditResult::unsafe_path(
                path.to_path_buf(),
                SecurityError::PathTooShallow {
                    path: path.to_path_buf(),
                    depth,
                    min: self.min_path_depth,
                },
            );
        }

        let canonical = match self.canonicalize_safe(path) {
            Ok(p) => p,
            Err(e) => return AuditResult::unsafe_path(path.to_path_buf(), e),
        };

        if self.hard_denylist.contains(&canonical) {
            return AuditResult::unsafe_path(
                path.to_path_buf(),
                SecurityError::HardDenylist(canonical),
            );
        }

        if self.is_protected_home_path(&canonical) {
            return AuditResult::unsafe_path(
                path.to_path_buf(),
                SecurityError::ProtectedPath(canonical),
            );
        }

        if self.is_protected_cache_path(&canonical) {
            return AuditResult::unsafe_path(
                path.to_path_buf(),
                SecurityError::ProtectedPath(canonical),
            );
        }

        if self.is_mount_point(&canonical) {
            return AuditResult::unsafe_path(
                path.to_path_buf(),
                SecurityError::MountPoint(canonical),
            );
        }

        let mut result = AuditResult::safe(canonical);

        if path.is_symlink() {
            result.warnings.push(format!(
                "Path is a symlink: {}",
                path.display()
            ));
        }

        result
    }

    /// Audit a path for deletion (includes ownership check)
    pub fn audit_for_deletion(&self, path: &Path) -> AuditResult {
        let mut result = self.audit(path);

        if !result.is_safe {
            return result;
        }

        // Check ownership
        if let Err(e) = self.check_ownership(&result.canonical_path) {
            result.is_safe = false;
            result.violations.push(e);
        }

        result
    }

    /// Re-verify a path immediately before deletion (TOCTOU mitigation).
    /// This minimizes the race window between the security audit and the
    /// actual filesystem operation by re-checking symlink status and ownership.
    pub fn verify_before_deletion(&self, path: &Path) -> Result<fs::Metadata, SecurityError> {
        // Re-check every path component, not just the final one. Otherwise an
        // attacker (or another process) can replace an intermediate directory
        // with a symlink between scanning and deletion.
        self.reject_symlink_components(path)?;
        let metadata = fs::symlink_metadata(path)
            .map_err(|_| SecurityError::CanonicalizationFailed(path.to_path_buf()))?;

        // Detect if the path became a symlink since the audit
        if metadata.file_type().is_symlink() {
            return Err(SecurityError::SymlinkEscape {
                original: path.to_path_buf(),
                resolved: fs::read_link(path).unwrap_or_default(),
            });
        }

        // Verify ownership hasn't changed
        if metadata.uid() != self.current_uid {
            return Err(SecurityError::NotOwned(path.to_path_buf()));
        }

        Ok(metadata)
    }

    /// Check if a path is owned by the current user
    fn check_ownership(&self, path: &Path) -> Result<(), SecurityError> {
        if !path.exists() {
            return Ok(());
        }

        let metadata = fs::symlink_metadata(path)
            .map_err(|_| SecurityError::NotOwned(path.to_path_buf()))?;

        // /tmp files may be owned by any user but we should only delete our own
        if metadata.uid() != self.current_uid {
            return Err(SecurityError::NotOwned(path.to_path_buf()));
        }

        Ok(())
    }

    /// Safely canonicalize a path, checking for symlink escapes
    fn canonicalize_safe(&self, path: &Path) -> Result<PathBuf, SecurityError> {
        self.reject_symlink_components(path)?;

        if !path.exists() {
            let mut ancestor = path.to_path_buf();
            while !ancestor.exists() {
                if let Some(parent) = ancestor.parent() {
                    ancestor = parent.to_path_buf();
                } else {
                    break;
                }
            }

            if ancestor.exists() {
                let canonical_ancestor = fs::canonicalize(&ancestor)
                    .map_err(|_| SecurityError::CanonicalizationFailed(path.to_path_buf()))?;

                let relative = path.strip_prefix(&ancestor).unwrap_or(path);
                return Ok(canonical_ancestor.join(relative));
            }

            return Ok(path.to_path_buf());
        }

        let canonical = fs::canonicalize(path)
            .map_err(|_| SecurityError::CanonicalizationFailed(path.to_path_buf()))?;

        Ok(canonical)
    }

    fn reject_symlink_components(&self, path: &Path) -> Result<(), SecurityError> {
        let mut current = PathBuf::new();
        for component in path.components() {
            current.push(component.as_os_str());
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    let resolved = fs::canonicalize(path)
                        .or_else(|_| fs::canonicalize(&current))
                        .unwrap_or_else(|_| {
                            fs::read_link(&current).unwrap_or_else(|_| current.clone())
                        });
                    return Err(SecurityError::SymlinkEscape {
                        original: path.to_path_buf(),
                        resolved,
                    });
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(_) => {
                    return Err(SecurityError::CanonicalizationFailed(path.to_path_buf()));
                }
            }
        }
        Ok(())
    }

    /// Check if path is in an allowed base directory
    fn is_in_allowed_base(&self, path: &Path) -> bool {
        for base in &self.allowed_user_bases {
            if path.starts_with(base) {
                return true;
            }
        }
        false
    }

    /// Check if path is a protected home directory path
    fn is_protected_home_path(&self, path: &Path) -> bool {
        for protected in &self.protected_home_paths {
            if path == protected || path.starts_with(protected) {
                return true;
            }
        }
        false
    }

    /// Check if path is a protected cache directory path
    fn is_protected_cache_path(&self, path: &Path) -> bool {
        for protected in &self.protected_cache_paths {
            if path == protected || path.starts_with(protected) {
                return true;
            }
        }
        false
    }

    /// Check if path is a mount point
    fn is_mount_point(&self, path: &Path) -> bool {
        // Simple heuristic: check if parent has different device ID
        if !path.exists() {
            return false;
        }

        let Ok(path_meta) = fs::metadata(path) else {
            return false;
        };

        if let Some(parent) = path.parent() {
            if let Ok(parent_meta) = fs::metadata(parent) {
                return path_meta.dev() != parent_meta.dev();
            }
        }

        // Root is always a mount point
        path == Path::new("/")
    }

}

impl Default for SecurityAuditor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hard_denylist() {
        let auditor = SecurityAuditor::new();

        // These should all be rejected
        let denied = [
            "/",
            "/usr",
            "/bin",
            "/etc",
            "/boot",
            "/lib",
            "/var",
        ];

        for path in denied {
            let result = auditor.audit(Path::new(path));
            assert!(!result.is_safe, "Path should be denied: {}", path);
        }
    }

    #[test]
    fn test_relative_path_rejected() {
        let auditor = SecurityAuditor::new();

        let result = auditor.audit(Path::new("./some/path"));
        assert!(!result.is_safe);

        let result = auditor.audit(Path::new("../etc/passwd"));
        assert!(!result.is_safe);
    }

    #[test]
    fn test_empty_path_rejected() {
        let auditor = SecurityAuditor::new();
        let result = auditor.audit(Path::new(""));
        assert!(!result.is_safe);
    }

    #[test]
    fn test_path_traversal_rejected() {
        let auditor = SecurityAuditor::new();

        let result = auditor.audit(Path::new("/home/user/../../../etc/passwd"));
        assert!(!result.is_safe);
    }

    #[test]
    fn test_shallow_path_rejected() {
        let auditor = SecurityAuditor::new();

        // Depth 2 should be rejected (default min is 3)
        let result = auditor.audit(Path::new("/nonexistent"));
        assert!(!result.is_safe);
    }

    #[test]
    fn test_cache_paths_allowed() {
        let auditor = SecurityAuditor::new();

        // These should be allowed (if they exist and have sufficient depth)
        if let Some(cache) = dirs::cache_dir() {
            let test_path = cache.join("some-app/cache");
            // Path might not exist, so we check the audit logic
            if test_path.components().count() >= 3 {
                let result = auditor.audit(&test_path);
                // Should not be rejected for security reasons
                // (might fail ownership check on real system)
                assert!(
                    result.is_safe || result.violations.iter().any(|v| {
                        matches!(v, SecurityError::CanonicalizationFailed(_))
                    }),
                    "Cache path should be allowed or fail gracefully"
                );
            }
        }
    }

    #[test]
    fn test_protected_home_paths() {
        let auditor = SecurityAuditor::new();

        if let Some(home) = dirs::home_dir() {
            let ssh = home.join(".ssh");
            let result = auditor.audit(&ssh);
            assert!(!result.is_safe, ".ssh should be protected");

            let gnupg = home.join(".gnupg");
            let result = auditor.audit(&gnupg);
            assert!(!result.is_safe, ".gnupg should be protected");

            let documents = home.join("Documents");
            let result = auditor.audit(&documents);
            assert!(!result.is_safe, "Documents should be protected");
        }
    }

    #[test]
    fn test_custom_rule_paths() {
        let auditor = SecurityAuditor::new();

        // Unsafe system paths that should never be allowed as custom rules
        let unsafe_system_paths = [
            "/etc/passwd",
            "/usr/bin",
            "/var/log",
            "/boot",
            "/",
            "/bin",
            "/lib",
        ];

        for path in &unsafe_system_paths {
            let result = auditor.audit(Path::new(path));
            assert!(!result.is_safe, "Path should be denied: {}", path);
        }

        // Home-relative unsafe paths (only test if home dir is available)
        if let Some(home) = dirs::home_dir() {
            let unsafe_home_paths = [
                home.join(".ssh"),
                home.join(".gnupg"),
                home.join("Documents"),
            ];

            for path in &unsafe_home_paths {
                let result = auditor.audit(path);
                assert!(!result.is_safe, "Path should be denied: {}", path.display());
            }

            // Safe paths under home
            let safe_paths = [
                home.join(".cache/myapp"),
                home.join(".local/share/myapp/tmp"),
                home.join(".config/myapp/old-data"),
            ];

            for path in &safe_paths {
                let result = auditor.audit(path);
                assert!(
                    result.is_safe || result.violations.iter().any(|v| matches!(v, SecurityError::CanonicalizationFailed(_))),
                    "Path should be allowed or fail gracefully: {}", path.display()
                );
            }
        }

        // Safe system paths
        let safe_system_paths = ["/tmp/myapp-cache"];
        for path in &safe_system_paths {
            let result = auditor.audit(Path::new(path));
            assert!(
                result.is_safe || result.violations.iter().any(|v| matches!(v, SecurityError::CanonicalizationFailed(_))),
                "Path should be allowed or fail gracefully: {}", path
            );
        }
    }

    #[test]
    fn protected_system_roots_reject_all_descendants() {
        let auditor = SecurityAuditor::new();
        for path in [
            "/root/projects/file.txt",
            "/var/lib/example/cache.bin",
            "/opt/example/data",
            "/srv/example/data",
            "/mnt/backup/archive",
            "/media/usb/DCIM/photo.jpg",
        ] {
            let result = auditor.audit(Path::new(path));
            assert!(!result.is_safe, "descendant should be denied: {path}");
        }
    }

    #[test]
    fn intermediate_symlink_components_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let outside = directory.path().join("outside");
        let root = directory.path().join("root");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(outside.join("important.txt"), b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        let result = SecurityAuditor::new().audit_for_deletion(&root.join("link/important.txt"));

        assert!(!result.is_safe);
        assert!(result
            .violations
            .iter()
            .any(|violation| matches!(violation, SecurityError::SymlinkEscape { .. })));
    }
}
