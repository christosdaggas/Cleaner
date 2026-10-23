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
    /// When set, instead of cleaning `path` directly, search every
    /// subdirectory of `path` for folders with this exact name and treat
    /// each match as a cleanup target (e.g., "target" build folders).
    #[serde(default)]
    pub subfolder_name: Option<String>,
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
            subfolder_name: None,
        }
    }

    /// True when this rule searches a directory tree for subfolders
    /// matching `subfolder_name` instead of cleaning `path` directly.
    pub fn is_subfolder_search(&self) -> bool {
        self.subfolder_name
            .as_deref()
            .map(str::trim)
            .is_some_and(|name| !name.is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_json_without_subfolder_name_still_loads() {
        let json = r#"{
            "id": "0198f6a2-1111-7222-8333-444455556666",
            "name": "Old rule",
            "description": "",
            "path": "/tmp/some-cache",
            "deletion_mode": "FilesOnly",
            "enabled": false,
            "file_pattern": null,
            "min_age_days": 0
        }"#;

        let rule: CustomRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.name, "Old rule");
        assert!(rule.subfolder_name.is_none());
        assert!(!rule.is_subfolder_search());
    }

    #[test]
    fn blank_subfolder_names_do_not_enable_search_mode() {
        let mut rule = CustomRule::new("Rule", "/tmp/x");
        assert!(!rule.is_subfolder_search());

        rule.subfolder_name = Some("   ".to_string());
        assert!(!rule.is_subfolder_search());

        rule.subfolder_name = Some("target".to_string());
        assert!(rule.is_subfolder_search());
    }
}
