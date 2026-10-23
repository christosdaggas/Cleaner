mod cleaner;
mod icon_resolver;
mod kernel_cleanup;
mod log_cleanup;
mod scanner;
mod security;
mod storage_analyzer;

pub use cleaner::{CleanOptions, Cleaner};
pub use icon_resolver::{DesktopEnvironment, DisplayServer, IconDiagnostics, IconResolver};
pub use kernel_cleanup::{build_plan, detect_manager, removal_command};
pub use log_cleanup::{system_journal_available, vacuum_system_journal};
pub use scanner::{ScanOptions, Scanner};
pub use security::SecurityAuditor;
pub use storage_analyzer::{
    analyze_storage, move_to_trash, StorageAnalysis, StorageAnalysisError, StorageNode,
    StorageScanOptions, TrashTarget,
};
