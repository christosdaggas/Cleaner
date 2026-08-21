//! Icon resolution with freedesktop fallback chain.

use gtk4::gdk;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IconResolution {
    pub icon_name: String,
    pub resolution_type: ResolutionType,
    pub original_request: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionType {
    ThemeIcon,
    FallbackIcon,
    BundledResource,
    GenericFallback,
}

impl ResolutionType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ThemeIcon => "Theme Icon",
            Self::FallbackIcon => "Fallback Icon",
            Self::BundledResource => "Bundled Resource",
            Self::GenericFallback => "Generic Fallback",
        }
    }

    pub fn is_fallback(&self) -> bool {
        !matches!(self, Self::ThemeIcon)
    }
}

/// Resolves icons using a fallback chain for cross-desktop compatibility.
#[derive(Debug, Clone)]
pub struct IconResolver {
    resolutions: Vec<IconResolution>,
}

impl IconResolver {
    pub fn new() -> Self {
        Self {
            resolutions: Vec::new(),
        }
    }

    pub fn resolve(&mut self, icon_name: &str) -> IconResolution {
        let resolution = self.resolve_internal(icon_name);
        self.resolutions.push(resolution.clone());
        resolution
    }

    fn resolve_internal(&self, icon_name: &str) -> IconResolution {
        if self.icon_exists_in_theme(icon_name) {
            debug!("Icon '{}' found in current theme", icon_name);
            return IconResolution {
                icon_name: icon_name.to_string(),
                resolution_type: ResolutionType::ThemeIcon,
                original_request: icon_name.to_string(),
            };
        }

        let fallbacks = Self::get_fallbacks(icon_name);
        for fallback in &fallbacks {
            if self.icon_exists_in_theme(fallback) {
                debug!(
                    "Icon '{}' not found, using fallback '{}'",
                    icon_name, fallback
                );
                return IconResolution {
                    icon_name: fallback.to_string(),
                    resolution_type: ResolutionType::FallbackIcon,
                    original_request: icon_name.to_string(),
                };
            }
        }

        if let Some(resource_path) = Self::get_bundled_resource(icon_name) {
            if gio::resources_get_info(&resource_path, gio::ResourceLookupFlags::NONE).is_ok() {
                debug!(
                    "Icon '{}' not in theme, using bundled resource '{}'",
                    icon_name, resource_path
                );
                return IconResolution {
                    icon_name: resource_path,
                    resolution_type: ResolutionType::BundledResource,
                    original_request: icon_name.to_string(),
                };
            }
        }

        let generic = Self::generic_fallback_icon();
        warn!(
            "Icon '{}' could not be resolved, using generic fallback '{}'",
            icon_name, generic
        );
        IconResolution {
            icon_name: generic,
            resolution_type: ResolutionType::GenericFallback,
            original_request: icon_name.to_string(),
        }
    }

    fn icon_exists_in_theme(&self, icon_name: &str) -> bool {
        let Some(display) = gdk::Display::default() else {
            return false;
        };

        let icon_theme = gtk4::IconTheme::for_display(&display);
        icon_theme.has_icon(icon_name)
    }

    fn get_fallbacks(icon_name: &str) -> Vec<&'static str> {
        match icon_name {
            // Browser icons
            "firefox" | "firefox-esr" => vec!["firefox-symbolic", "web-browser", "web-browser-symbolic"],
            "google-chrome" | "chrome" => vec!["google-chrome-symbolic", "chromium", "web-browser"],
            "chromium" | "chromium-browser" => vec!["chromium-symbolic", "google-chrome", "web-browser"],
            "brave" | "brave-browser" => vec!["brave-symbolic", "web-browser"],
            "microsoft-edge" => vec!["microsoft-edge-symbolic", "web-browser"],
            "opera" => vec!["opera-symbolic", "web-browser"],
            "vivaldi" => vec!["vivaldi-symbolic", "web-browser"],

            // System icons
            "folder-symbolic" => vec!["folder", "inode-directory"],
            "user-trash-symbolic" => vec!["user-trash", "trash-empty"],
            "dialog-error-symbolic" => vec!["dialog-error", "error"],
            "computer-symbolic" => vec!["computer", "system"],
            "package-x-generic-symbolic" => vec!["package-x-generic", "package"],
            "image-x-generic-symbolic" => vec!["image-x-generic", "image"],
            "folder-documents-symbolic" => vec!["folder-documents", "folder"],

            // Security icons
            "security-high-symbolic" => vec!["security-high", "dialog-password", "system-lock-screen"],
            "security-medium-symbolic" => vec!["security-medium", "dialog-warning"],
            "security-low-symbolic" => vec!["security-low", "dialog-error"],

            // Action icons
            "edit-delete-symbolic" => vec!["edit-delete", "user-trash"],
            "view-refresh-symbolic" => vec!["view-refresh", "reload"],
            "document-save-symbolic" => vec!["document-save", "save"],

            _ => {
                if icon_name.ends_with("-symbolic") {
                    vec!["application-x-executable-symbolic"]
                } else {
                    vec!["application-x-executable"]
                }
            }
        }
    }

    fn get_bundled_resource(icon_name: &str) -> Option<String> {
        let browser_icons = [
            ("firefox", "/com/chrisdaggas/datacleaner/icons/browsers/firefox.svg"),
            ("google-chrome", "/com/chrisdaggas/datacleaner/icons/browsers/chrome.svg"),
            ("chromium", "/com/chrisdaggas/datacleaner/icons/browsers/chromium.svg"),
            ("brave", "/com/chrisdaggas/datacleaner/icons/browsers/brave.svg"),
            ("microsoft-edge", "/com/chrisdaggas/datacleaner/icons/browsers/edge.svg"),
            ("opera", "/com/chrisdaggas/datacleaner/icons/browsers/opera.svg"),
            ("vivaldi", "/com/chrisdaggas/datacleaner/icons/browsers/vivaldi.svg"),
            ("yandex-browser", "/com/chrisdaggas/datacleaner/icons/browsers/yandex.svg"),
        ];

        for (name, path) in browser_icons {
            if icon_name == name || icon_name.starts_with(name) {
                return Some(path.to_string());
            }
        }

        None
    }

    fn generic_fallback_icon() -> String {
        let generics = [
            "application-x-executable",
            "application-x-executable-symbolic",
            "system-run",
            "application-default-icon",
        ];
        generics[0].to_string()
    }

    pub fn get_diagnostics_summary(&self) -> IconDiagnostics {
        let total = self.resolutions.len();
        let theme_resolved = self.resolutions.iter().filter(|r| r.resolution_type == ResolutionType::ThemeIcon).count();
        let fallback_used = self.resolutions.iter().filter(|r| r.resolution_type == ResolutionType::FallbackIcon).count();
        let bundled_used = self.resolutions.iter().filter(|r| r.resolution_type == ResolutionType::BundledResource).count();
        let generic_used = self.resolutions.iter().filter(|r| r.resolution_type == ResolutionType::GenericFallback).count();

        IconDiagnostics {
            total_resolutions: total,
            theme_resolved,
            fallback_used,
            bundled_used,
            generic_used,
            current_theme: Self::get_current_theme_name(),
        }
    }

    pub fn get_current_theme_name() -> Option<String> {
        gdk::Display::default().map(|display| {
            let theme = gtk4::IconTheme::for_display(&display);
            theme.theme_name().to_string()
        })
    }
}

impl Default for IconResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct IconDiagnostics {
    pub total_resolutions: usize,
    pub theme_resolved: usize,
    pub fallback_used: usize,
    pub bundled_used: usize,
    pub generic_used: usize,
    pub current_theme: Option<String>,
}

impl IconDiagnostics {
    pub fn success_rate(&self) -> f64 {
        if self.total_resolutions == 0 {
            return 100.0;
        }
        let non_generic = self.total_resolutions - self.generic_used;
        (non_generic as f64 / self.total_resolutions as f64) * 100.0
    }

    pub fn all_resolved(&self) -> bool {
        self.generic_used == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopEnvironment {
    Gnome,
    Kde,
    Cosmic,
    Xfce,
    Cinnamon,
    Mate,
    Budgie,
    Pantheon,
    Lxqt,
    Lxde,
    Unknown(String),
}

impl DesktopEnvironment {
    pub fn detect() -> Self {
        if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
            let desktop_lower = desktop.to_lowercase();

            if desktop_lower.contains("gnome") { return Self::Gnome; }
            if desktop_lower.contains("kde") || desktop_lower.contains("plasma") { return Self::Kde; }
            if desktop_lower.contains("cosmic") { return Self::Cosmic; }
            if desktop_lower.contains("xfce") { return Self::Xfce; }
            if desktop_lower.contains("cinnamon") { return Self::Cinnamon; }
            if desktop_lower.contains("mate") { return Self::Mate; }
            if desktop_lower.contains("budgie") { return Self::Budgie; }
            if desktop_lower.contains("pantheon") { return Self::Pantheon; }
            if desktop_lower.contains("lxqt") { return Self::Lxqt; }
            if desktop_lower.contains("lxde") { return Self::Lxde; }

            return Self::Unknown(desktop);
        }

        if let Ok(session) = std::env::var("DESKTOP_SESSION") {
            let session_lower = session.to_lowercase();

            if session_lower.contains("gnome") { return Self::Gnome; }
            if session_lower.contains("plasma") || session_lower.contains("kde") { return Self::Kde; }
            if session_lower.contains("cosmic") { return Self::Cosmic; }

            return Self::Unknown(session);
        }

        Self::Unknown("Unknown".to_string())
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Gnome => "GNOME",
            Self::Kde => "KDE Plasma",
            Self::Cosmic => "COSMIC",
            Self::Xfce => "Xfce",
            Self::Cinnamon => "Cinnamon",
            Self::Mate => "MATE",
            Self::Budgie => "Budgie",
            Self::Pantheon => "Pantheon",
            Self::Lxqt => "LXQt",
            Self::Lxde => "LXDE",
            Self::Unknown(name) => name,
        }
    }

    pub fn is_wayland() -> bool {
        std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE")
                .map(|s| s == "wayland")
                .unwrap_or(false)
    }

    pub fn display_server() -> DisplayServer {
        if Self::is_wayland() {
            DisplayServer::Wayland
        } else if std::env::var("DISPLAY").is_ok() {
            DisplayServer::X11
        } else {
            DisplayServer::Unknown
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    Wayland,
    X11,
    Unknown,
}

impl DisplayServer {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Wayland => "Wayland",
            Self::X11 => "X11",
            Self::Unknown => "Unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_chain() {
        assert!(!IconResolver::get_fallbacks("firefox").is_empty());
        assert!(!IconResolver::get_fallbacks("google-chrome").is_empty());
        assert!(!IconResolver::get_fallbacks("folder-symbolic").is_empty());
    }

    #[test]
    fn test_bundled_resources() {
        assert!(IconResolver::get_bundled_resource("firefox").is_some());
        assert!(IconResolver::get_bundled_resource("google-chrome").is_some());
        assert!(IconResolver::get_bundled_resource("chromium").is_some());
        assert!(IconResolver::get_bundled_resource("folder-symbolic").is_none());
    }

    #[test]
    fn test_desktop_environment_detection() {
        let de = DesktopEnvironment::detect();
        assert!(!de.display_name().is_empty());
    }

    #[test]
    fn test_display_server_detection() {
        let ds = DesktopEnvironment::display_server();
        assert!(!ds.display_name().is_empty());
    }

    #[test]
    fn test_icon_diagnostics() {
        let resolver = IconResolver::new();
        let diags = resolver.get_diagnostics_summary();

        assert_eq!(diags.total_resolutions, 0);
        assert_eq!(diags.success_rate(), 100.0);
        assert!(diags.all_resolved());
    }

    #[test]
    fn test_generic_fallback() {
        let generic = IconResolver::generic_fallback_icon();
        assert!(!generic.is_empty());
        assert!(generic.contains("application") || generic.contains("system"));
    }
}
