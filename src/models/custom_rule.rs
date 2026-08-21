use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DeletionMode {
    #[default]
    FilesOnly,
    FilesAndDirectories,
}

impl DeletionMode {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::FilesOnly => "Files Only",
            Self::FilesAndDirectories => "Files and Directories",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::FilesOnly => "Delete files but keep folder structure",
            Self::FilesAndDirectories => "Delete everything including folders",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRule {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub deletion_mode: DeletionMode,
    pub enabled: bool,
    /// Optional file pattern filter (e.g., "*.log", "*.tmp")
    pub file_pattern: Option<String>,
    /// Minimum file age in days (0 = no restriction)
    pub min_age_days: u32,
}

impl CustomRule {
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: String::new(),
            path: path.into(),
            deletion_mode: DeletionMode::FilesOnly,
            enabled: false,
            file_pattern: None,
            min_age_days: 0,
        }
    }

    pub fn expanded_path(&self) -> Option<PathBuf> {
        let path_str = self.path.to_string_lossy();
        if path_str.starts_with("~/") {
            dirs::home_dir().map(|home| home.join(&path_str[2..]))
        } else if path_str == "~" {
            dirs::home_dir()
        } else {
            Some(self.path.clone())
        }
    }
}
