use std::path::PathBuf;
use std::process::Command;

fn first_existing(candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

fn journal_tools() -> Option<(PathBuf, PathBuf)> {
    let pkexec = first_existing(&["/usr/bin/pkexec", "/bin/pkexec"])?;
    let journalctl = first_existing(&["/usr/bin/journalctl", "/bin/journalctl"])?;
    Some((pkexec, journalctl))
}

pub fn system_journal_available() -> bool {
    journal_tools().is_some()
}

/// Vacuum archived systemd journal files through PolicyKit. `journalctl`
/// preserves the active journal and applies the requested age cutoff itself;
/// Data Cleaner never removes files directly from `/var/log`.
pub fn vacuum_system_journal(retention_days: u32) -> Result<String, String> {
    let (pkexec, journalctl) = journal_tools().ok_or_else(|| {
        "System journal cleanup requires pkexec and journalctl on the host".to_string()
    })?;
    let retention = format!("--vacuum-time={}d", retention_days.clamp(1, 3650));

    let output = Command::new(pkexec)
        .arg(journalctl)
        .arg(retention)
        .output()
        .map_err(|error| format!("Could not start journal cleanup: {error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(if !stdout.is_empty() {
            stdout
        } else if !stderr.is_empty() {
            stderr
        } else {
            "System journal cleanup completed".to_string()
        })
    } else if stderr.is_empty() {
        Err(format!(
            "System journal cleanup failed with status {}",
            output.status
        ))
    } else {
        Err(stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_existing_ignores_missing_candidates() {
        assert_eq!(
            first_existing(&["/definitely/missing/data-cleaner-command"]),
            None
        );
    }

    #[test]
    fn paths_must_point_to_regular_files() {
        assert_eq!(
            first_existing(&[std::path::Path::new("/").to_str().unwrap()]),
            None
        );
    }
}
