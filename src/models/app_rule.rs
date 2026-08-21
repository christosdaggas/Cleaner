use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRule {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    /// Path to clean (can contain ~ for home directory)
    pub path: PathBuf,
    pub enabled: bool,
    pub builtin: bool,
}

impl AppRule {
    pub fn new(name: impl Into<String>, description: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            path: path.into(),
            enabled: false,
            builtin: false,
        }
    }

    pub fn builtin(name: impl Into<String>, description: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let mut rule = Self::new(name, description, path);
        rule.builtin = true;
        rule
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

    pub fn defaults() -> Vec<Self> {
        vec![
            Self::builtin(
                "NPM Cache",
                "Node.js package manager cache",
                "~/.npm/_cacache",
            ),
            Self::builtin(
                "NPM Logs",
                "Node.js package manager logs",
                "~/.npm/_logs",
            ),
            Self::builtin(
                "Yarn Cache",
                "Yarn package manager cache",
                "~/.cache/yarn",
            ),
            Self::builtin(
                "Yarn Logs",
                "Yarn package manager logs",
                "~/.cache/yarn/logs",
            ),
            Self::builtin(
                "pnpm Cache",
                "pnpm package manager cache",
                "~/.cache/pnpm",
            ),
            Self::builtin(
                "pip Cache",
                "Python package manager cache",
                "~/.cache/pip",
            ),
            Self::builtin(
                "Poetry Cache",
                "Python Poetry cache",
                "~/.cache/pypoetry",
            ),
            Self::builtin(
                "Cargo Cache",
                "Rust package manager cache (registry only)",
                "~/.cargo/registry/cache",
            ),
            Self::builtin(
                "Maven Cache",
                "Java Maven repository cache",
                "~/.m2/repository",
            ),
            Self::builtin(
                "Gradle Cache",
                "Java Gradle build cache",
                "~/.gradle/caches",
            ),
            Self::builtin(
                "Go Mod Cache",
                "Go module cache",
                "~/go/pkg/mod/cache",
            ),
            Self::builtin(
                "Go Build Cache",
                "Go compiler build cache",
                "~/.cache/go-build",
            ),
            Self::builtin(
                "Bun Cache",
                "Bun package manager cache",
                "~/.bun/install/cache",
            ),
            Self::builtin(
                "pre-commit Cache",
                "pre-commit hook environment cache",
                "~/.cache/pre-commit",
            ),
            Self::builtin(
                "VS Code Cache",
                "Visual Studio Code cache",
                "~/.config/Code/Cache",
            ),
            Self::builtin(
                "VS Code CachedData",
                "Visual Studio Code cached data",
                "~/.config/Code/CachedData",
            ),
            Self::builtin(
                "JetBrains Cache",
                "JetBrains IDE caches",
                "~/.cache/JetBrains",
            ),
            Self::builtin(
                "Spotify Cache",
                "Spotify streaming cache",
                "~/.cache/spotify",
            ),
            Self::builtin(
                "Discord Cache",
                "Discord application cache",
                "~/.config/discord/Cache",
            ),
            Self::builtin(
                "Discord Code Cache",
                "Discord JavaScript bundle cache",
                "~/.config/discord/Code Cache",
            ),
            Self::builtin(
                "Discord GPU Cache",
                "Discord GPU shader cache",
                "~/.config/discord/GPUCache",
            ),
            Self::builtin(
                "Slack Cache",
                "Slack application cache",
                "~/.config/Slack/Cache",
            ),
            Self::builtin(
                "Slack Code Cache",
                "Slack JavaScript bundle cache",
                "~/.config/Slack/Code Cache",
            ),
            Self::builtin(
                "Slack GPU Cache",
                "Slack GPU shader cache",
                "~/.config/Slack/GPUCache",
            ),
            Self::builtin(
                "Teams Cache",
                "Microsoft Teams cache",
                "~/.config/Microsoft/Microsoft Teams/Cache",
            ),
            Self::builtin(
                "Teams Code Cache",
                "Microsoft Teams JavaScript bundle cache",
                "~/.config/Microsoft/Microsoft Teams/Code Cache",
            ),
            Self::builtin(
                "Teams GPU Cache",
                "Microsoft Teams GPU shader cache",
                "~/.config/Microsoft/Microsoft Teams/GPUCache",
            ),
            Self::builtin(
                "Flatpak VS Code Cache",
                "Visual Studio Code Flatpak cache",
                "~/.var/app/com.visualstudio.code/cache",
            ),
            Self::builtin(
                "Flatpak Discord Cache",
                "Discord Flatpak cache",
                "~/.var/app/com.discordapp.Discord/cache",
            ),
            Self::builtin(
                "Flatpak Slack Cache",
                "Slack Flatpak cache",
                "~/.var/app/com.slack.Slack/cache",
            ),
            Self::builtin(
                "Flatpak Spotify Cache",
                "Spotify Flatpak cache",
                "~/.var/app/com.spotify.Client/cache",
            ),
            Self::builtin(
                "Flatpak Firefox Cache",
                "Firefox Flatpak cache",
                "~/.var/app/org.mozilla.firefox/cache",
            ),
            Self::builtin(
                "Flatpak Chromium Cache",
                "Chromium Flatpak cache",
                "~/.var/app/org.chromium.Chromium/cache",
            ),
            Self::builtin(
                "User State Logs",
                "Top-level XDG state log files",
                "~/.local/state/*.log",
            ),
            Self::builtin(
                "Application State Logs",
                "Per-application XDG state log files",
                "~/.local/state/*/*.log",
            ),
            Self::builtin(
                "User Journal Logs",
                "Systemd user journal logs",
                "~/.local/share/systemd/journal",
            ),
        ]
    }
}
