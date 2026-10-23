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
            Self::Firefox => &[
                "firefox.desktop",
                "firefox_firefox.desktop",
                "org.mozilla.firefox.desktop",
            ],
            Self::Chrome => &[
                "google-chrome.desktop",
                "google-chrome-stable.desktop",
                "com.google.Chrome.desktop",
            ],
            Self::Chromium => &[
                "chromium.desktop",
                "chromium-browser.desktop",
                "chromium_chromium.desktop",
                "org.chromium.Chromium.desktop",
            ],
            Self::Brave => &[
                "brave-browser.desktop",
                "brave-browser-stable.desktop",
                "com.brave.Browser.desktop",
            ],
            Self::Edge => &[
                "microsoft-edge.desktop",
                "microsoft-edge-stable.desktop",
                "com.microsoft.Edge.desktop",
            ],
            Self::Opera => &["opera.desktop", "com.opera.Opera.desktop"],
            Self::Vivaldi => &[
                "vivaldi.desktop",
                "vivaldi-stable.desktop",
                "com.vivaldi.Vivaldi.desktop",
            ],
            Self::Yandex => &[
                "yandex-browser.desktop",
                "yandex-browser-stable.desktop",
                "ru.yandex.Browser.desktop",
            ],
            Self::DuckDuckGo => &[
                "duckduckgo.desktop",
                "com.duckduckgo.DesktopBrowser.desktop",
            ],
        }
    }

    /// Directory name the browser uses under an XDG cache or config home.
    /// Chromium-based browsers use the same name in both places.
    fn data_dir_name(&self) -> &'static str {
        match self {
            Self::Firefox => "mozilla/firefox",
            Self::Chrome => "google-chrome",
            Self::Chromium => "chromium",
            Self::Brave => "BraveSoftware/Brave-Browser",
            Self::Edge => "microsoft-edge",
            Self::Opera => "opera",
            Self::Vivaldi => "vivaldi",
            Self::Yandex => "yandex-browser",
            Self::DuckDuckGo => "duckduckgo",
        }
    }

    /// Flatpak application ID. Flatpak apps keep their data under
    /// `~/.var/app/<id>/` instead of the host XDG directories.
    fn flatpak_id(&self) -> &'static str {
        match self {
            Self::Firefox => "org.mozilla.firefox",
            Self::Chrome => "com.google.Chrome",
            Self::Chromium => "org.chromium.Chromium",
            Self::Brave => "com.brave.Browser",
            Self::Edge => "com.microsoft.Edge",
            Self::Opera => "com.opera.Opera",
            Self::Vivaldi => "com.vivaldi.Vivaldi",
            Self::Yandex => "ru.yandex.Browser",
            Self::DuckDuckGo => "com.duckduckgo.DesktopBrowser",
        }
    }

    /// Every location that can hold this browser's cache: the native XDG
    /// cache, the Flatpak sandbox cache, and the Snap cache where one exists.
    /// The native location is always first.
    pub fn cache_roots(&self) -> Vec<PathBuf> {
        let name = self.data_dir_name();
        let mut roots = Vec::new();
        if let Some(cache) = dirs::cache_dir() {
            roots.push(cache.join(name));
        }
        if let Some(home) = dirs::home_dir() {
            roots.push(
                home.join(".var/app")
                    .join(self.flatpak_id())
                    .join("cache")
                    .join(name),
            );
            match self {
                Self::Firefox => roots.push(home.join("snap/firefox/common/.cache").join(name)),
                Self::Chromium => roots.push(home.join("snap/chromium/common/.cache").join(name)),
                _ => {}
            }
        }
        roots
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

        if let Some(data_dirs) = std::env::var_os("XDG_DATA_DIRS") {
            data_roots.extend(std::env::split_paths(&data_dirs));
        } else {
            data_roots.extend([
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]);
        }

        // System-wide Flatpak and Snap installs.
        data_roots.extend([
            PathBuf::from("/var/lib/flatpak/exports/share"),
            PathBuf::from("/var/lib/snapd/desktop"),
        ]);

        data_roots.into_iter().any(|root| {
            let applications = root.join("applications");
            self.desktop_file_names()
                .iter()
                .any(|name| applications.join(name).is_file())
        })
    }

    /// Every location that can hold this browser's profiles: native, Flatpak,
    /// and Snap where one exists. The native location is always first.
    pub fn config_roots(&self) -> Vec<PathBuf> {
        let Some(home) = dirs::home_dir() else {
            return Vec::new();
        };
        let flatpak = home.join(".var/app").join(self.flatpak_id());

        match self {
            Self::Firefox => vec![
                home.join(".mozilla/firefox"),
                flatpak.join(".mozilla/firefox"),
                home.join("snap/firefox/common/.mozilla/firefox"),
            ],
            _ => {
                let name = self.data_dir_name();
                let mut roots = Vec::new();
                if let Some(config) = dirs::config_dir() {
                    roots.push(config.join(name));
                }
                roots.push(flatpak.join("config").join(name));
                if *self == Self::Chromium {
                    roots.push(home.join("snap/chromium/common/chromium"));
                }
                roots
            }
        }
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
            Self::Firefox => &["firefox", "firefox-bin", "firefox-esr"],
            Self::Chrome => &["chrome"],
            // Snap Chromium's processes are named `chrome`.
            Self::Chromium => &["chromium", "chromium-browser", "chrome"],
            Self::Brave => &["brave"],
            Self::Edge => &["msedge"],
            Self::Opera => &["opera"],
            Self::Vivaldi => &["vivaldi", "vivaldi-bin"],
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
        // Map every known process name to its browsers, case-insensitive. A
        // name can belong to several browsers (`chrome`); all of them are then
        // treated as running, which only ever skips cleanup.
        let mut name_to_browsers: HashMap<String, Vec<BrowserType>> = HashMap::new();
        for browser in Self::all() {
            for name in browser.process_names() {
                name_to_browsers
                    .entry(Self::comm_key(name))
                    .or_default()
                    .push(*browser);
            }
        }

        let mut running: HashSet<BrowserType> = HashSet::new();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return running;
        };

        for entry in entries.flatten() {
            // Stop early if we've found every browser we know about.
            if running.len() == Self::all().len() {
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
                if let Some(browsers) = name_to_browsers.get(&Self::comm_key(comm.trim())) {
                    running.extend(browsers.iter().copied());
                }
            }
        }

        running
    }

    /// The kernel truncates `/proc/<pid>/comm` to 15 bytes, so
    /// `chromium-browser` shows up as `chromium-browse`.
    fn comm_key(name: &str) -> String {
        const COMM_LEN: usize = 15;
        let name = name.to_ascii_lowercase();
        let mut end = name.len().min(COMM_LEN);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        name[..end].to_string()
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
            Self::SiteData => {
                "Offline website data and local storage. May sign you out of some sites."
            }
            Self::CrashReports => "Diagnostic crash files. Safe to delete.",
            Self::History => "Record of visited websites. Cannot be recovered.",
            Self::Cookies => "Login sessions and preferences. Will log you out of websites.",
            Self::DownloadHistory => {
                "List of downloaded files. Does not delete the files themselves."
            }
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
                Self::CrashReports => {
                    &["Crash Reports", "Crashpad", "*/Crash Reports", "*/Crashpad"]
                }
                Self::History => &["History", "*/History"],
                Self::Cookies => &[
                    "Cookies",
                    "Network/Cookies",
                    "*/Cookies",
                    "*/Network/Cookies",
                ],
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

    /// Every root this rule may scan. A custom path replaces the defaults;
    /// otherwise the native, Flatpak, and Snap locations are all candidates,
    /// with the native location first.
    pub fn effective_paths(&self) -> Vec<PathBuf> {
        if let Some(custom) = &self.custom_path {
            return vec![custom.clone()];
        }
        if self.data_type == BrowserDataType::Cache {
            self.browser.cache_roots()
        } else {
            self.browser.config_roots()
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
        format!(
            "{} - {}",
            self.browser.display_name(),
            self.data_type.display_name()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_names_match_truncated_comm() {
        assert_eq!(
            BrowserType::comm_key("chromium-browser"),
            BrowserType::comm_key("chromium-browse")
        );
        assert_eq!(BrowserType::comm_key("Firefox"), "firefox");
        assert!(BrowserType::Vivaldi
            .process_names()
            .contains(&"vivaldi-bin"));
        assert!(BrowserType::Chromium.process_names().contains(&"chrome"));
    }

    #[test]
    fn every_browser_includes_its_flatpak_locations() {
        let home = dirs::home_dir().expect("tests need a home directory");
        for browser in BrowserType::all() {
            let sandbox = home.join(".var/app").join(browser.flatpak_id());
            assert!(
                browser
                    .cache_roots()
                    .iter()
                    .any(|root| root.starts_with(sandbox.join("cache"))),
                "{browser:?} is missing its Flatpak cache root"
            );
            assert!(
                browser
                    .config_roots()
                    .iter()
                    .any(|root| root.starts_with(&sandbox)),
                "{browser:?} is missing its Flatpak profile root"
            );
        }
    }

    #[test]
    fn native_locations_stay_first() {
        let home = dirs::home_dir().expect("tests need a home directory");
        assert_eq!(
            BrowserType::Firefox.config_roots()[0],
            home.join(".mozilla/firefox")
        );
        let cache = dirs::cache_dir().expect("tests need a cache directory");
        assert_eq!(
            BrowserType::Chrome.cache_roots()[0],
            cache.join("google-chrome")
        );
    }

    #[test]
    fn snap_locations_are_known_for_firefox_and_chromium() {
        let home = dirs::home_dir().expect("tests need a home directory");
        assert!(BrowserType::Firefox
            .config_roots()
            .contains(&home.join("snap/firefox/common/.mozilla/firefox")));
        assert!(BrowserType::Chromium
            .config_roots()
            .contains(&home.join("snap/chromium/common/chromium")));
    }

    #[test]
    fn a_custom_path_replaces_every_default_location() {
        let mut rule = BrowserRule::new(BrowserType::Chrome, BrowserDataType::Cookies);
        rule.custom_path = Some(PathBuf::from("/srv/profiles/chrome"));
        assert_eq!(
            rule.effective_paths(),
            vec![PathBuf::from("/srv/profiles/chrome")]
        );
    }
}
