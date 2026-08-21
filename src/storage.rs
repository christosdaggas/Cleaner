//! Persistent storage for application configuration and rules.

use crate::models::{AppRule, AppSettings, BrowserRule, BrowserType, CustomRule, SystemRule};
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::RwLock;
use thiserror::Error;
use tracing::{debug, error, warn};

const CONFIG_DIR_NAME: &str = "data-cleaner";
const LEGACY_CONFIG_DIR_NAME: &str = "cleaner";
const CONFIG_FILE_NAMES: &[&str] = &[
    "settings.json",
    "browser_rules.json",
    "app_rules.json",
    "custom_rules.json",
    "system_rules.json",
];

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Lock poisoned: {context}")]
    LockPoisoned { context: String },

    #[error("Failed to read file '{path}': {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to write file '{path}': {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse JSON from '{path}': {source}")]
    ParseJson {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("Failed to serialize to JSON: {0}")]
    SerializeJson(#[from] serde_json::Error),

    #[error("Failed to create config directory '{path}': {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub type StorageResult<T> = Result<T, StorageError>;

pub trait Enableable {
    fn is_enabled(&self) -> bool;
}

impl Enableable for BrowserRule {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Enableable for AppRule {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Enableable for CustomRule {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Enableable for SystemRule {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

pub struct Storage {
    config_dir: PathBuf,
    settings: RwLock<AppSettings>,
    browser_rules: RwLock<Vec<BrowserRule>>,
    app_rules: RwLock<Vec<AppRule>>,
    custom_rules: RwLock<Vec<CustomRule>>,
    system_rules: RwLock<Vec<SystemRule>>,
}

impl Storage {
    pub fn new() -> Self {
        let config_dir = match dirs::config_dir() {
            Some(dir) => {
                let current = dir.join(CONFIG_DIR_NAME);
                Self::migrate_legacy_config(&dir.join(LEGACY_CONFIG_DIR_NAME), &current);
                current
            }
            None => match dirs::home_dir() {
                Some(home) => {
                    warn!("Config directory not available; falling back to ~/.data-cleaner");
                    let current = home.join(".data-cleaner");
                    Self::migrate_legacy_config(&home.join(".cleaner"), &current);
                    current
                }
                None => {
                    warn!("Config directory and home directory not available; falling back to current directory .data-cleaner");
                    let current = PathBuf::from(".data-cleaner");
                    Self::migrate_legacy_config(&PathBuf::from(".cleaner"), &current);
                    current
                }
            }
        };

        if !config_dir.exists() {
            if let Err(e) = fs::create_dir_all(&config_dir) {
                warn!("Failed to create config directory: {}", e);
            }
        }

        let storage = Self {
            config_dir: config_dir.clone(),
            settings: RwLock::new(AppSettings::default()),
            browser_rules: RwLock::new(BrowserRule::defaults()),
            app_rules: RwLock::new(AppRule::defaults()),
            custom_rules: RwLock::new(Vec::new()),
            system_rules: RwLock::new(SystemRule::defaults()),
        };

        storage.load_all_with_logging();
        storage
    }

    fn migrate_legacy_config(legacy_dir: &std::path::Path, config_dir: &std::path::Path) {
        if config_dir.exists() || !legacy_dir.is_dir() {
            return;
        }

        match fs::rename(legacy_dir, config_dir) {
            Ok(()) => {
                tracing::info!(
                    from = %legacy_dir.display(),
                    to = %config_dir.display(),
                    "Migrated Data Cleaner configuration"
                );
                return;
            }
            Err(error) => {
                warn!(
                    "Could not move legacy configuration from {} to {}: {}. Falling back to copying known settings files.",
                    legacy_dir.display(),
                    config_dir.display(),
                    error
                );
            }
        }

        if let Err(error) = fs::create_dir_all(config_dir) {
            warn!(
                "Could not create migrated configuration directory {}: {}",
                config_dir.display(),
                error
            );
            return;
        }

        for file_name in CONFIG_FILE_NAMES {
            let source = legacy_dir.join(file_name);
            let destination = config_dir.join(file_name);
            if source.is_file() {
                if let Err(error) = fs::copy(&source, &destination) {
                    warn!(
                        "Could not migrate {} to {}: {}",
                        source.display(),
                        destination.display(),
                        error
                    );
                }
            }
        }
    }

    fn load_all_with_logging(&self) {
        if let Err(e) = self.load_settings() {
            Self::log_load_fallback("settings", &e);
        }
        if let Err(e) = self.load_browser_rules() {
            Self::log_load_fallback("browser rules", &e);
        }
        if let Err(e) = self.load_app_rules() {
            Self::log_load_fallback("application rules", &e);
        }
        if let Err(e) = self.load_custom_rules() {
            Self::log_load_fallback("custom rules", &e);
        }
        if let Err(e) = self.load_system_rules() {
            Self::log_load_fallback("system rules", &e);
        }
    }

    fn log_load_fallback(context: &str, error: &StorageError) {
        if matches!(
            error,
            StorageError::ReadFile { source, .. }
                if source.kind() == std::io::ErrorKind::NotFound
        ) {
            debug!("No saved {context}; using defaults");
        } else {
            warn!("Could not load {context}; using defaults: {error}");
        }
    }

    fn load_into<T: DeserializeOwned>(
        &self,
        path: &PathBuf,
        lock: &RwLock<T>,
        context: &str,
    ) -> StorageResult<()> {
        let data = fs::read_to_string(path).map_err(|e| StorageError::ReadFile {
            path: path.clone(),
            source: e,
        })?;

        let parsed: T = match serde_json::from_str(&data) {
            Ok(parsed) => parsed,
            Err(source) => {
                let backup = path.with_extension(format!(
                    "json.corrupt-{}",
                    uuid::Uuid::new_v4()
                ));
                match fs::copy(path, &backup) {
                    Ok(_) => warn!(
                        "Preserved unreadable configuration {} as {}",
                        path.display(),
                        backup.display()
                    ),
                    Err(error) => warn!(
                        "Could not preserve unreadable configuration {}: {}",
                        path.display(),
                        error
                    ),
                }
                return Err(StorageError::ParseJson {
                    path: path.clone(),
                    source,
                });
            }
        };

        let mut guard = lock.write().map_err(|_| StorageError::LockPoisoned {
            context: context.to_string(),
        })?;
        *guard = parsed;

        debug!("Loaded {} from {:?}", context, path);
        Ok(())
    }

    fn save_from<T: Serialize>(
        &self,
        path: &PathBuf,
        lock: &RwLock<T>,
        context: &str,
    ) -> StorageResult<()> {
        let guard = lock.read().map_err(|_| StorageError::LockPoisoned {
            context: context.to_string(),
        })?;

        let data = serde_json::to_string_pretty(&*guard)?;
        drop(guard);

        let parent = path.parent().ok_or_else(|| StorageError::WriteFile {
            path: path.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "configuration path has no parent",
            ),
        })?;
        fs::create_dir_all(parent).map_err(|e| StorageError::WriteFile {
            path: path.clone(),
            source: e,
        })?;

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.json");
        let temporary_path = parent.join(format!(
            ".{file_name}.{}.tmp",
            uuid::Uuid::new_v4()
        ));

        let write_result = (|| -> std::io::Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)?;
            file.write_all(data.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary_path, path)?;
            if let Ok(directory) = fs::File::open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();

        if let Err(source) = write_result {
            let _ = fs::remove_file(&temporary_path);
            return Err(StorageError::WriteFile {
                path: path.clone(),
                source,
            });
        }

        debug!("Saved {} to {:?}", context, path);
        Ok(())
    }

    fn get_clone<T: Clone>(&self, lock: &RwLock<T>, context: &str) -> StorageResult<T> {
        lock.read()
            .map(|guard| guard.clone())
            .map_err(|_| StorageError::LockPoisoned {
                context: context.to_string(),
            })
    }

    fn update_and_save<T: Clone + Serialize, F>(
        &self,
        lock: &RwLock<T>,
        path: &PathBuf,
        context: &str,
        updater: F,
    ) -> StorageResult<()>
    where
        F: FnOnce(&mut T),
    {
        let previous = {
            let mut guard = lock.write().map_err(|_| StorageError::LockPoisoned {
                context: context.to_string(),
            })?;
            let previous = guard.clone();
            updater(&mut guard);
            previous
        };
        if let Err(error) = self.save_from(path, lock, context) {
            if let Ok(mut guard) = lock.write() {
                *guard = previous;
            }
            return Err(error);
        }
        Ok(())
    }

    fn count_enabled<T: Enableable>(&self, lock: &RwLock<Vec<T>>, context: &str) -> usize {
        lock.read()
            .map(|guard| guard.iter().filter(|r| r.is_enabled()).count())
            .unwrap_or_else(|e| {
                error!("Lock poisoned while counting {}: {}", context, e);
                0
            })
    }

    fn settings_path(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    fn load_settings(&self) -> StorageResult<()> {
        self.load_into(&self.settings_path(), &self.settings, "settings")
    }

    pub fn save_settings(&self) -> StorageResult<()> {
        self.save_from(&self.settings_path(), &self.settings, "settings")
    }

    pub fn get_settings(&self) -> AppSettings {
        let mut settings = self.get_clone(&self.settings, "settings")
            .unwrap_or_else(|e| {
                error!("Failed to read settings: {}", e);
                AppSettings::default()
            });
        settings.clamp_values();
        settings
    }

    pub fn update_settings<F>(&self, f: F) -> StorageResult<()>
    where
        F: FnOnce(&mut AppSettings),
    {
        self.update_and_save(&self.settings, &self.settings_path(), "settings", f)
    }

    fn browser_rules_path(&self) -> PathBuf {
        self.config_dir.join("browser_rules.json")
    }

    fn load_browser_rules(&self) -> StorageResult<()> {
        self.load_into(
            &self.browser_rules_path(),
            &self.browser_rules,
            "browser_rules",
        )?;

        // Reconcile saved rules with the current browser/data-type catalog so
        // upgrades gain new cleanup choices without discarding prior choices.
        // Rules for browsers no longer detected are switched off.
        let installed: std::collections::HashSet<BrowserType> = BrowserType::all()
            .iter()
            .copied()
            .filter(BrowserType::is_installed)
            .collect();
        let defaults = BrowserRule::defaults();
        let mut changed = false;

        {
            let mut rules = self.browser_rules.write().map_err(|_| StorageError::LockPoisoned {
                context: "browser_rules".to_string(),
            })?;

            for rule in rules.iter_mut() {
                if rule.enabled && !installed.contains(&rule.browser) {
                    rule.enabled = false;
                    changed = true;
                }
            }

            for default_rule in defaults {
                let exists = rules.iter().any(|rule| {
                    rule.browser == default_rule.browser
                        && rule.data_type == default_rule.data_type
                });
                if !exists {
                    rules.push(default_rule);
                    changed = true;
                }
            }
        }

        if changed {
            self.save_browser_rules()?;
        }

        Ok(())
    }

    pub fn save_browser_rules(&self) -> StorageResult<()> {
        self.save_from(
            &self.browser_rules_path(),
            &self.browser_rules,
            "browser_rules",
        )
    }

    pub fn get_browser_rules(&self) -> Vec<BrowserRule> {
        self.get_clone(&self.browser_rules, "browser_rules")
            .unwrap_or_else(|e| {
                error!("Failed to read browser rules: {}", e);
                BrowserRule::defaults()
            })
    }

    pub fn update_browser_rules<F>(&self, f: F) -> StorageResult<()>
    where
        F: FnOnce(&mut Vec<BrowserRule>),
    {
        self.update_and_save(
            &self.browser_rules,
            &self.browser_rules_path(),
            "browser_rules",
            f,
        )
    }

    fn app_rules_path(&self) -> PathBuf {
        self.config_dir.join("app_rules.json")
    }

    fn load_app_rules(&self) -> StorageResult<()> {
        self.load_into(&self.app_rules_path(), &self.app_rules, "app_rules")
    }

    pub fn save_app_rules(&self) -> StorageResult<()> {
        self.save_from(&self.app_rules_path(), &self.app_rules, "app_rules")
    }

    pub fn get_app_rules(&self) -> Vec<AppRule> {
        self.get_clone(&self.app_rules, "app_rules")
            .unwrap_or_else(|e| {
                error!("Failed to read app rules: {}", e);
                AppRule::defaults()
            })
    }

    pub fn update_app_rules<F>(&self, f: F) -> StorageResult<()>
    where
        F: FnOnce(&mut Vec<AppRule>),
    {
        self.update_and_save(&self.app_rules, &self.app_rules_path(), "app_rules", f)
    }

    fn custom_rules_path(&self) -> PathBuf {
        self.config_dir.join("custom_rules.json")
    }

    fn load_custom_rules(&self) -> StorageResult<()> {
        self.load_into(
            &self.custom_rules_path(),
            &self.custom_rules,
            "custom_rules",
        )
    }

    pub fn save_custom_rules(&self) -> StorageResult<()> {
        self.save_from(
            &self.custom_rules_path(),
            &self.custom_rules,
            "custom_rules",
        )
    }

    pub fn get_custom_rules(&self) -> Vec<CustomRule> {
        self.get_clone(&self.custom_rules, "custom_rules")
            .unwrap_or_else(|e| {
                error!("Failed to read custom rules: {}", e);
                Vec::new()
            })
    }

    pub fn update_custom_rules<F>(&self, f: F) -> StorageResult<()>
    where
        F: FnOnce(&mut Vec<CustomRule>),
    {
        self.update_and_save(
            &self.custom_rules,
            &self.custom_rules_path(),
            "custom_rules",
            f,
        )
    }

    fn system_rules_path(&self) -> PathBuf {
        self.config_dir.join("system_rules.json")
    }

    fn load_system_rules(&self) -> StorageResult<()> {
        self.load_into(
            &self.system_rules_path(),
            &self.system_rules,
            "system_rules",
        )
    }

    pub fn save_system_rules(&self) -> StorageResult<()> {
        self.save_from(
            &self.system_rules_path(),
            &self.system_rules,
            "system_rules",
        )
    }

    pub fn get_system_rules(&self) -> Vec<SystemRule> {
        self.get_clone(&self.system_rules, "system_rules")
            .unwrap_or_else(|e| {
                error!("Failed to read system rules: {}", e);
                SystemRule::defaults()
            })
    }

    pub fn update_system_rules<F>(&self, f: F) -> StorageResult<()>
    where
        F: FnOnce(&mut Vec<SystemRule>),
    {
        self.update_and_save(
            &self.system_rules,
            &self.system_rules_path(),
            "system_rules",
            f,
        )
    }

    pub fn count_enabled_rules(&self) -> usize {
        let settings = self.get_settings();
        self.count_enabled(&self.browser_rules, "browser_rules")
            + self.count_enabled(&self.app_rules, "app_rules")
            + self.count_enabled(&self.custom_rules, "custom_rules")
            + self.count_enabled(&self.system_rules, "system_rules")
            + usize::from(settings.application_log_cleanup_enabled)
            + usize::from(settings.system_journal_cleanup_enabled)
    }

    pub fn config_dir(&self) -> &PathBuf {
        &self.config_dir
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_storage() -> (Storage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage {
            config_dir: temp_dir.path().to_path_buf(),
            settings: RwLock::new(AppSettings::default()),
            browser_rules: RwLock::new(Vec::new()),
            app_rules: RwLock::new(Vec::new()),
            custom_rules: RwLock::new(Vec::new()),
            system_rules: RwLock::new(Vec::new()),
        };
        (storage, temp_dir)
    }

    #[test]
    fn test_settings_roundtrip() {
        let (storage, _dir) = create_test_storage();

        storage
            .update_settings(|s| {
                s.confirm_before_clean = false;
                s.max_files_per_operation = 5000;
            })
            .unwrap();

        let settings = storage.get_settings();
        assert!(!settings.confirm_before_clean);
        assert_eq!(settings.max_files_per_operation, 5000);
        let entries: Vec<_> = fs::read_dir(storage.config_dir())
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert!(entries.iter().all(|entry| !entry
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }

    #[test]
    fn corrupt_configuration_is_preserved_for_recovery() {
        let (storage, _dir) = create_test_storage();
        fs::write(storage.settings_path(), "{truncated").unwrap();

        assert!(storage.load_settings().is_err());
        let backups: Vec<_> = fs::read_dir(storage.config_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings.json.corrupt-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), "{truncated");
    }

    #[test]
    fn test_custom_rules_crud() {
        let (storage, _dir) = create_test_storage();

        storage
            .update_custom_rules(|rules| {
                rules.push(CustomRule::new("Test Rule", "/tmp/test"));
            })
            .unwrap();

        let rules = storage.get_custom_rules();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "Test Rule");

        let rule_id = rules[0].id;
        storage
            .update_custom_rules(|rules| {
                if let Some(rule) = rules.iter_mut().find(|r| r.id == rule_id) {
                    rule.enabled = true;
                }
            })
            .unwrap();

        assert_eq!(storage.count_enabled_rules(), 1);

        storage
            .update_custom_rules(|rules| {
                rules.retain(|r| r.id != rule_id);
            })
            .unwrap();

        assert!(storage.get_custom_rules().is_empty());
    }

    #[test]
    fn test_load_from_disk() {
        let (storage, dir) = create_test_storage();

        storage
            .update_settings(|s| {
                s.verbose_logging = true;
            })
            .unwrap();

        let storage2 = Storage {
            config_dir: dir.path().to_path_buf(),
            settings: RwLock::new(AppSettings::default()),
            browser_rules: RwLock::new(Vec::new()),
            app_rules: RwLock::new(Vec::new()),
            custom_rules: RwLock::new(Vec::new()),
            system_rules: RwLock::new(Vec::new()),
        };
        storage2.load_all_with_logging();

        let settings = storage2.get_settings();
        assert!(settings.verbose_logging);
    }

    #[test]
    fn test_count_enabled_rules() {
        let (storage, _dir) = create_test_storage();

        storage
            .update_browser_rules(|rules| {
                rules.push(BrowserRule::defaults().into_iter().next().unwrap());
                if let Some(r) = rules.first_mut() {
                    r.enabled = true;
                }
            })
            .unwrap();

        storage
            .update_app_rules(|rules| {
                rules.push(AppRule::defaults().into_iter().next().unwrap());
                if let Some(r) = rules.first_mut() {
                    r.enabled = true;
                }
            })
            .unwrap();

        assert_eq!(storage.count_enabled_rules(), 2);
    }

    #[test]
    fn test_concurrent_access() {
        let (storage, _dir) = create_test_storage();
        let storage = Arc::new(storage);

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let storage = Arc::clone(&storage);
                std::thread::spawn(move || {
                    storage
                        .update_custom_rules(|rules| {
                            rules.push(CustomRule::new(format!("Rule {}", i), "/tmp"));
                        })
                        .unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(storage.get_custom_rules().len(), 10);
    }

    #[test]
    fn legacy_config_directory_is_migrated() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join(LEGACY_CONFIG_DIR_NAME);
        let current = root.path().join(CONFIG_DIR_NAME);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), "legacy-settings").unwrap();

        Storage::migrate_legacy_config(&legacy, &current);

        assert!(!legacy.exists());
        assert_eq!(
            fs::read_to_string(current.join("settings.json")).unwrap(),
            "legacy-settings"
        );
    }
}
