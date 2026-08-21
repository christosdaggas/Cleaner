pub struct ReleaseNotes {
    pub version: &'static str,
    pub date: &'static str,
    pub title: &'static str,
    pub changes: &'static [&'static str],
}

/// The in-app changelog. Add future releases at the beginning of this list so
/// the newest changes always appear first in the dialog.
pub const RELEASES: &[ReleaseNotes] = &[ReleaseNotes {
    version: "1.0.0",
    date: "21 August 2026",
    title: "Initial public release",
    changes: &[
        "A modern GNOME interface with unified light and dark surfaces, system accent colors, high contrast, and reduced motion support.",
        "A dashboard storage overview for used, available, total, and reclaimable disk space.",
        "Safe browser cleanup with installed-browser detection and unavailable browsers clearly disabled.",
        "Application cache, system, custom directory, application log, and system journal cleanup with configurable retention.",
        "Scheduled automatic cleanup with multiple cleanup days, a chosen local time, and optional start at login.",
        "A lightweight visual Storage Analyzer for finding large files and folders and moving selected items to Trash.",
        "An in-app cleanup log showing deleted, skipped, and failed items after every cleanup.",
        "System tray integration with an adaptive symbolic icon and quick cleanup actions.",
        "A startup update check with direct access to the latest verified GitHub release.",
        "Safety limits, critical-path protection, symlink traversal prevention, previews, and running-browser checks.",
        "Consistent Data Cleaner application, executable, package, desktop, and configuration naming.",
        "English, Greek, Italian, Spanish, German, and French interface languages with automatic system-language detection.",
    ],
}];
