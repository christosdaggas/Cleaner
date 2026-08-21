use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemRuleType {
    /// System temporary directory (/tmp)
    TmpDirectory,
    /// User cache directory (~/.cache)
    UserCache,
    /// Thumbnail cache
    Thumbnails,
    /// Trash/Recycle bin
    Trash,
    /// Crash reports
    CrashReports,
    /// Old kernel versions (requires root)
    OldKernels,
    /// Package manager cache
    PackageCache,
}

impl SystemRuleType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::TmpDirectory => "Temporary Files (/tmp)",
            Self::UserCache => "User Cache (~/.cache)",
            Self::Thumbnails => "Thumbnail Cache",
            Self::Trash => "Trash",
            Self::CrashReports => "Crash Reports",
            Self::OldKernels => "Old Kernels",
            Self::PackageCache => "Package Manager Cache",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::TmpDirectory => "Temporary files in /tmp owned by current user",
            Self::UserCache => "Application caches in ~/.cache",
            Self::Thumbnails => "Cached image thumbnails",
            Self::Trash => "Files in the trash bin",
            Self::CrashReports => "User-session crash reports and logs",
            Self::OldKernels => "Old Linux kernel versions (requires root)",
            Self::PackageCache => "Downloaded package files (apt, dnf, pacman, zypper)",
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::TmpDirectory => "folder-documents-symbolic",
            Self::UserCache => "folder-symbolic",
            Self::Thumbnails => "image-x-generic-symbolic",
            Self::Trash => "user-trash-symbolic",
            Self::CrashReports => "dialog-error-symbolic",
            Self::OldKernels => "computer-symbolic",
            Self::PackageCache => "package-x-generic-symbolic",
        }
    }

    pub fn requires_root(&self) -> bool {
        matches!(self, Self::OldKernels | Self::PackageCache)
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        match self {
            Self::TmpDirectory => vec![PathBuf::from("/tmp")],
            Self::UserCache => {
                dirs::cache_dir()
                    .map(|p| vec![p])
                    .unwrap_or_default()
            }
            Self::Thumbnails => {
                dirs::cache_dir()
                    .map(|cache| vec![cache.join("thumbnails")])
                    .unwrap_or_default()
            }
            Self::Trash => {
                dirs::data_local_dir()
                    .map(|data| vec![data.join("Trash/files"), data.join("Trash/info")])
                    .unwrap_or_default()
            }
            Self::CrashReports => {
                let mut paths = Vec::new();
                if let Some(home) = dirs::home_dir() {
                    paths.push(home.join(".xsession-errors"));
                }
                paths
            }
            Self::OldKernels => vec![], // Handled specially
            Self::PackageCache => vec![
                PathBuf::from("/var/cache/apt/archives"),
                PathBuf::from("/var/cache/dnf"),
                PathBuf::from("/var/cache/pacman/pkg"),
                PathBuf::from("/var/cache/zypp/packages"),
            ],
        }
    }

    pub fn all() -> &'static [SystemRuleType] {
        &[
            Self::TmpDirectory,
            Self::UserCache,
            Self::Thumbnails,
            Self::Trash,
            Self::CrashReports,
        ]
    }

    pub fn root_rules() -> &'static [SystemRuleType] {
        &[Self::OldKernels, Self::PackageCache]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRule {
    pub id: Uuid,
    pub rule_type: SystemRuleType,
    pub enabled: bool,
    pub custom_paths: Option<Vec<PathBuf>>,
}

impl SystemRule {
    pub fn new(rule_type: SystemRuleType) -> Self {
        Self {
            id: Uuid::new_v4(),
            rule_type,
            enabled: false,
            custom_paths: None,
        }
    }

    pub fn effective_paths(&self) -> Vec<PathBuf> {
        self.custom_paths
            .clone()
            .unwrap_or_else(|| self.rule_type.paths())
    }

    pub fn display_name(&self) -> &'static str {
        self.rule_type.display_name()
    }

    pub fn description(&self) -> &'static str {
        self.rule_type.description()
    }

    pub fn requires_root(&self) -> bool {
        self.rule_type.requires_root()
    }

    pub fn defaults() -> Vec<Self> {
        SystemRuleType::all()
            .iter()
            .map(|t| Self::new(*t))
            .collect()
    }
}
