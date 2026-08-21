use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserType {
    Firefox,
    Chrome,
    Chromium,
    Brave,
    Edge,
    Opera,
    Vivaldi,
    Yandex,
    DuckDuckGo,
}

impl BrowserType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Firefox => "Firefox",
            Self::Chrome => "Google Chrome",
            Self::Chromium => "Chromium",
            Self::Brave => "Brave",
            Self::Edge => "Microsoft Edge",
            Self::Opera => "Opera",
            Self::Vivaldi => "Vivaldi",
            Self::Yandex => "Yandex Browser",
            Self::DuckDuckGo => "DuckDuckGo",
        }
    }

    pub fn icon_resource(&self) -> &'static str {
        match self {
            Self::Firefox => "/com/chrisdaggas/datacleaner/icons/browsers/firefox.svg",
            Self::Chrome => "/com/chrisdaggas/datacleaner/icons/browsers/chrome.svg",
            Self::Chromium => "/com/chrisdaggas/datacleaner/icons/browsers/chromium.svg",
            Self::Brave => "/com/chrisdaggas/datacleaner/icons/browsers/brave.svg",
            Self::Edge => "/com/chrisdaggas/datacleaner/icons/browsers/edge.svg",
            Self::Opera => "/com/chrisdaggas/datacleaner/icons/browsers/opera.svg",
            Self::Vivaldi => "/com/chrisdaggas/datacleaner/icons/browsers/vivaldi.svg",
            Self::Yandex => "/com/chrisdaggas/datacleaner/icons/browsers/yandex.svg",
            Self::DuckDuckGo => "/com/chrisdaggas/datacleaner/icons/browsers/duckduckgo.svg",
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::Firefox => "firefox",
            Self::Chrome => "google-chrome",
            Self::Chromium => "chromium",
            Self::Brave => "brave",
            Self::Edge => "microsoft-edge",
            Self::Opera => "opera",
            Self::Vivaldi => "vivaldi",
            Self::Yandex => "yandex-browser",
            Self::DuckDuckGo => "web-browser",
        }
    }

    fn executable_names(&self) -> &'static [&'static str] {
        match self {
            Self::Firefox => &["firefox", "firefox-esr"],
            Self::Chrome => &["google-chrome", "google-chrome-stable"],
            Self::Chromium => &["chromium", "chromium-browser"],
            Self::Brave => &["brave-browser", "brave-browser-stable"],
            Self::Edge => &["microsoft-edge", "microsoft-edge-stable"],
            Self::Opera => &["opera"],
            Self::Vivaldi => &["vivaldi", "vivaldi-stable"],
            Self::Yandex => &["yandex-browser", "yandex-browser-stable"],
            Self::DuckDuckGo => &["duckduckgo"],
        }
    }

    fn desktop_file_names(&self) -> &'static [&'static str] {
        match self {
            Self::Firefox => &["firefox.desktop", "firefox_firefox.desktop", "org.mozilla.firefox.desktop"],
            Self::Chrome => &["google-chrome.desktop", "google-chrome-stable.desktop", "com.google.Chrome.desktop"],
            Self::Chromium => &["chromium.desktop", "chromium-browser.desktop", "chromium_chromium.desktop", "org.chromium.Chromium.desktop"],
            Self::Brave => &["brave-browser.desktop", "brave-browser-stable.desktop", "com.brave.Browser.desktop"],
            Self::Edge => &["microsoft-edge.desktop", "microsoft-edge-stable.desktop", "com.microsoft.Edge.desktop"],
            Self::Opera => &["opera.desktop", "com.opera.Opera.desktop"],
            Self::Vivaldi => &["vivaldi.desktop", "vivaldi-stable.desktop", "com.vivaldi.Vivaldi.desktop"],
            Self::Yandex => &["yandex-browser.desktop", "yandex-browser-stable.desktop", "ru.yandex.Browser.desktop"],
            Self::DuckDuckGo => &["duckduckgo.desktop", "com.duckduckgo.DesktopBrowser.desktop"],
        }
    }

    pub fn cache_path(&self) -> Option<PathBuf> {
        let cache = dirs::cache_dir()?;

        Some(match self {
            Self::Firefox => cache.join("mozilla/firefox"),
            Self::Chrome => cache.join("google-chrome"),
            Self::Chromium => cache.join("chromium"),
            Self::Brave => cache.join("BraveSoftware/Brave-Browser"),
            Self::Edge => cache.join("microsoft-edge"),
            Self::Opera => cache.join("opera"),
            Self::Vivaldi => cache.join("vivaldi"),
            Self::Yandex => cache.join("yandex-browser"),
            Self::DuckDuckGo => cache.join("duckduckgo"),
        })
    }

    pub fn is_installed(&self) -> bool {
        // Deliberately require an executable or launcher. A stale profile by
        // itself must not make an uninstalled browser appear in Settings.
        self.has_executable() || self.has_desktop_entry()
    }

    fn has_executable(&self) -> bool {
        std::env::var_os("PATH")
            .map(|path| {
                std::env::split_paths(&path).any(|directory| {
                    self.executable_names()
                        .iter()
                        .any(|name| directory.join(name).is_file())
                })
            })
            .unwrap_or(false)
    }

    fn has_desktop_entry(&self) -> bool {
        let mut data_roots = Vec::new();
        if let Some(data_home) = dirs::data_dir() {
            data_roots.push(data_home.clone());
            data_roots.push(data_home.join("flatpak/exports/share"));
        }
        if let Some(home) = dirs::home_dir() {
            // Explicit host-user locations also work when XDG data paths are
            // remapped inside a Flatpak sandbox.
            data_roots.push(home.join(".local/share"));
            data_roots.push(home.join(".local/share/flatpak/exports/share"));
        }

        if let Some(data_dirs) = std::env::var_os("XDG_DATA_DIRS") {
            data_roots.extend(std::env::split_paths(&data_dirs));
        } else {
            data_roots.extend([
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]);
        }

        // Native, Flatpak, Snap, and host paths visible from a Flatpak.
        data_roots.extend([
            PathBuf::from("/var/lib/flatpak/exports/share"),
            PathBuf::from("/var/lib/snapd/desktop"),
            PathBuf::from("/run/host/usr/local/share"),
            PathBuf::from("/run/host/usr/share"),
            PathBuf::from("/run/host/var/lib/flatpak/exports/share"),
            PathBuf::from("/run/host/var/lib/snapd/desktop"),
        ]);

        data_roots.into_iter().any(|root| {
            let applications = root.join("applications");
            self.desktop_file_names()
                .iter()
                .any(|name| applications.join(name).is_file())
        })
    }

    pub fn config_path(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let config = dirs::config_dir()?;

        Some(match self {
            Self::Firefox => home.join(".mozilla/firefox"),
            Self::Chrome => config.join("google-chrome"),
            Self::Chromium => config.join("chromium"),
            Self::Brave => config.join("BraveSoftware/Brave-Browser"),
            Self::Edge => config.join("microsoft-edge"),
            Self::Opera => config.join("opera"),
            Self::Vivaldi => config.join("vivaldi"),
            Self::Yandex => config.join("yandex-browser"),
            Self::DuckDuckGo => config.join("duckduckgo"),
        })
    }

    pub fn all() -> &'static [BrowserType] {
        &[
            Self::Firefox,
            Self::Chrome,
            Self::Chromium,
            Self::Brave,
            Self::Edge,
            Self::Opera,
            Self::Vivaldi,
            Self::Yandex,
            Self::DuckDuckGo,
        ]
    }

    /// Process names used to detect if this browser is currently running
    pub fn process_names(&self) -> &'static [&'static str] {
        match self {
            Self::Firefox => &["firefox"],
            Self::Chrome => &["chrome"],
            Self::Chromium => &["chromium"],
            Self::Brave => &["brave"],
            Self::Edge => &["msedge"],
            Self::Opera => &["opera"],
            Self::Vivaldi => &["vivaldi"],
            Self::Yandex => &["yandex_browser"],
            Self::DuckDuckGo => &["duckduckgo"],
        }
    }

    /// Check if this browser has a running process by scanning /proc
    pub fn is_running(&self) -> bool {
        Self::running_browsers().contains(self)
    }

    /// Scan /proc once and return the set of all browser types that are
    /// currently running. Callers that need to check multiple browsers must
    /// use this instead of calling `is_running()` per browser to avoid
    /// re-scanning /proc N times.
    pub fn running_browsers() -> HashSet<BrowserType> {
        // Map every known process name to its browser, case-insensitive.
        let mut name_to_browser: HashMap<String, BrowserType> = HashMap::new();
        for browser in Self::all() {
            for name in browser.process_names() {
                name_to_browser.insert((*name).to_ascii_lowercase(), *browser);
            }
        }

        let mut running: HashSet<BrowserType> = HashSet::new();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return running;
        };

        for entry in entries.flatten() {
            // Stop early if we've found every browser we know about.
            if running.len() == name_to_browser.values().collect::<HashSet<_>>().len() {
                break;
            }

            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Only check numeric directories (PIDs)
            if !name_str.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }

            let comm_path = entry.path().join("comm");
            if let Ok(comm) = std::fs::read_to_string(&comm_path) {
                let key = comm.trim().to_ascii_lowercase();
                if let Some(browser) = name_to_browser.get(&key) {
                    running.insert(*browser);
                }
            }
        }

        running
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserDataType {
    Cache,
    SiteData,
    CrashReports,
    History,
    Cookies,
    DownloadHistory,
}

impl BrowserDataType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Cache => "Cache",
            Self::SiteData => "Website Storage",
            Self::CrashReports => "Crash Reports",
            Self::History => "History",
            Self::Cookies => "Cookies",
            Self::DownloadHistory => "Download History",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Cache => "Temporary files stored locally. Safe to delete.",
            Self::SiteData => "Offline website data and local storage. May sign you out of some sites.",
            Self::CrashReports => "Diagnostic crash files. Safe to delete.",
            Self::History => "Record of visited websites. Cannot be recovered.",
            Self::Cookies => "Login sessions and preferences. Will log you out of websites.",
            Self::DownloadHistory => "List of downloaded files. Does not delete the files themselves.",
        }
    }

    pub fn is_sensitive(&self) -> bool {
        matches!(self, Self::Cookies | Self::History | Self::SiteData)
    }

    pub fn recurse_directories(&self) -> bool {
        matches!(self, Self::Cache | Self::SiteData | Self::CrashReports)
    }

    pub fn relative_paths(&self, browser: BrowserType) -> &'static [&'static str] {
        match browser {
            BrowserType::Firefox => match self {
                Self::Cache => &["*/cache2", "*/startupCache"],
                Self::SiteData => &["*/storage/default", "*/storage/temporary"],
                Self::CrashReports => &["Crash Reports/pending", "*/minidumps"],
                Self::History => &["*/places.sqlite"],
                Self::Cookies => &["*/cookies.sqlite"],
                Self::DownloadHistory => &["*/downloads.sqlite"],
            },
            BrowserType::Chrome
            | BrowserType::Chromium
            | BrowserType::Brave
            | BrowserType::Edge
            | BrowserType::Opera
            | BrowserType::Vivaldi
            | BrowserType::Yandex
            | BrowserType::DuckDuckGo => match self {
                Self::Cache => &[
                    "Cache",
                    "*/Cache",
                    "Code Cache",
                    "*/Code Cache",
                    "GPUCache",
                    "*/GPUCache",
                    "GrShaderCache",
                    "*/GrShaderCache",
                    "GraphiteDawnCache",
                    "*/GraphiteDawnCache",
                ],
                Self::SiteData => &[
                    "Local Storage",
                    "Session Storage",
                    "IndexedDB",
                    "Service Worker",
                    "WebStorage",
                    "*/Local Storage",
                    "*/Session Storage",
                    "*/IndexedDB",
                    "*/Service Worker",
                    "*/WebStorage",
                ],
                Self::CrashReports => &["Crash Reports", "Crashpad", "*/Crash Reports", "*/Crashpad"],
                Self::History => &["History", "*/History"],
                Self::Cookies => &["Cookies", "Network/Cookies", "*/Cookies", "*/Network/Cookies"],
                Self::DownloadHistory => &["History", "*/History"],
            },
        }
    }

    pub fn all() -> &'static [BrowserDataType] {
        &[
            Self::Cache,
            Self::SiteData,
            Self::CrashReports,
            Self::History,
            Self::Cookies,
            Self::DownloadHistory,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserRule {
    pub id: Uuid,
    pub browser: BrowserType,
    pub data_type: BrowserDataType,
    pub enabled: bool,
    pub custom_path: Option<PathBuf>,
}

impl BrowserRule {
    pub fn new(browser: BrowserType, data_type: BrowserDataType) -> Self {
        Self {
            id: Uuid::new_v4(),
            browser,
            data_type,
            enabled: false,
            custom_path: None,
        }
    }

    pub fn effective_path(&self) -> Option<PathBuf> {
        if let Some(ref custom) = self.custom_path {
            Some(custom.clone())
        } else {
            if matches!(self.data_type, BrowserDataType::Cache) {
                self.browser.cache_path()
            } else {
                self.browser.config_path()
            }
        }
    }

    pub fn defaults() -> Vec<Self> {
        let mut rules = Vec::new();
        for browser in BrowserType::all() {
            let installed = browser.is_installed();
            for data_type in BrowserDataType::all() {
                let mut rule = Self::new(*browser, *data_type);
                rule.enabled = installed && *data_type == BrowserDataType::Cache;
                rules.push(rule);
            }
        }
        rules
    }

    pub fn display_name(&self) -> String {
        format!("{} - {}", self.browser.display_name(), self.data_type.display_name())
    }
}
