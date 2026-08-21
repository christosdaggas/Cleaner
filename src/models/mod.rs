mod settings;
mod browser_rule;
mod app_rule;
mod custom_rule;
mod system_rule;
mod scan_result;

pub use settings::{AppLanguage, AppSettings, ColorScheme, ScheduleDay};
pub use browser_rule::{BrowserRule, BrowserType, BrowserDataType};
pub use app_rule::AppRule;
pub use custom_rule::{CustomRule, DeletionMode};
pub use system_rule::{SystemRule, SystemRuleType};
pub use scan_result::{ScanResult, CleanResult, FileEntry, CleanError};
