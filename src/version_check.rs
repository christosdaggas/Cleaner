use serde::Deserialize;
use tracing::{debug, warn};

const GITHUB_OWNER: &str = "christosdaggas";
const GITHUB_REPO: &str = "Cleaner";

/// Information about a newer published release.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    /// Latest published version.
    pub latest_version: String,
    /// Release URL.
    pub download_url: String,
    #[allow(dead_code)]
    pub release_name: String,
}

/// Minimal GitHub release payload used by update checks.
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    name: Option<String>,
}

/// Returns the latest published release when it is newer than the local version.
pub async fn check_for_update(current_version: &str) -> Option<UpdateInfo> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        GITHUB_OWNER, GITHUB_REPO
    );

    debug!("Checking for updates at {}", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(format!("{}/{}", GITHUB_REPO, current_version))
        .build()
        .ok()?;

    let response = match client.get(&url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            debug!("Update check HTTP request failed (not an error): {}", e);
            return None;
        }
    };

    if !response.status().is_success() {
        debug!(
            "Update check got HTTP {} (repo may not have releases yet)",
            response.status()
        );
        return None;
    }

    let release: GitHubRelease = match response.json().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to parse GitHub release JSON: {}", e);
            return None;
        }
    };

    let latest = release
        .tag_name
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string();

    debug!("Update check: local={}, remote={}", current_version, latest);

    if is_newer(&latest, current_version) {
        Some(UpdateInfo {
            latest_version: latest,
            download_url: release.html_url,
            release_name: release.name.unwrap_or_default(),
        })
    } else {
        debug!("Application is up to date");
        None
    }
}

/// Returns `true` when `remote` sorts newer than `local`.
fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|part| {
                let numeric: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
                numeric.parse::<u64>().unwrap_or(0)
            })
            .collect()
    };

    let r = parse(remote);
    let l = parse(local);

    let max_len = r.len().max(l.len());
    for i in 0..max_len {
        let rv = r.get(i).copied().unwrap_or(0);
        let lv = l.get(i).copied().unwrap_or(0);
        if rv > lv {
            return true;
        }
        if rv < lv {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(is_newer("1.1.0", "1.0.0"));
        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
        assert!(is_newer("1.0.0.1", "1.0.0"));
        assert!(is_newer("1.1.0-beta.1", "1.0.0"));
    }
}
