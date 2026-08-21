use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const AUTOSTART_FILE: &str = "com.chrisdaggas.datacleaner.desktop";
const LEGACY_AUTOSTART_FILE: &str = "com.chrisdaggas.cleaner.desktop";

pub fn is_enabled() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let current = autostart_path_in_home(&home);
    if current.is_file() {
        return true;
    }

    let legacy = legacy_autostart_path_in_home(&home);
    if !legacy.is_file() {
        return false;
    }

    // Preserve an existing startup preference while changing both the
    // desktop ID and executable name.
    if let Err(error) = set_enabled_in_home(&home, true, is_flatpak()) {
        tracing::warn!("Could not migrate the legacy autostart entry: {error}");
    }
    true
}

pub fn set_enabled(enabled: bool) -> io::Result<()> {
    let home = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not locate the home directory",
        )
    })?;
    set_enabled_in_home(&home, enabled, is_flatpak())
}

fn autostart_path_in_home(home: &Path) -> PathBuf {
    home.join(".config").join("autostart").join(AUTOSTART_FILE)
}

fn legacy_autostart_path_in_home(home: &Path) -> PathBuf {
    home.join(".config")
        .join("autostart")
        .join(LEGACY_AUTOSTART_FILE)
}

fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").is_file()
}

fn set_enabled_in_home(home: &Path, enabled: bool, flatpak: bool) -> io::Result<()> {
    let path = autostart_path_in_home(home);
    let legacy_path = legacy_autostart_path_in_home(home);
    if !enabled {
        for candidate in [path, legacy_path] {
            match fs::remove_file(candidate) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        return Ok(());
    }

    let parent = path.parent().expect("autostart path always has a parent");
    fs::create_dir_all(parent)?;

    let exec = if flatpak {
        "flatpak run com.chrisdaggas.datacleaner --background"
    } else {
        "data-cleaner --background"
    };
    let desktop_entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Data Cleaner\n\
         Comment=Start Data Cleaner at login for scheduled cleanup\n\
         Exec={exec}\n\
         Icon=com.chrisdaggas.datacleaner\n\
         Terminal=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n"
    );

    let temporary_path = parent.join(format!(".{AUTOSTART_FILE}.{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temporary_path, desktop_entry)?;
    match fs::rename(&temporary_path, &path) {
        Ok(()) => {
            if legacy_path != path {
                let _ = fs::remove_file(legacy_path);
            }
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(temporary_path);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_creates_and_disable_removes_native_entry() {
        let home = tempfile::tempdir().unwrap();
        let legacy_path = legacy_autostart_path_in_home(home.path());
        fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        fs::write(&legacy_path, "Exec=cleaner --background\n").unwrap();

        set_enabled_in_home(home.path(), true, false).unwrap();

        let path = autostart_path_in_home(home.path());
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("Name=Data Cleaner"));
        assert!(contents.contains("Exec=data-cleaner --background"));
        assert!(!contents.contains("flatpak run"));
        assert!(!legacy_path.exists());

        set_enabled_in_home(home.path(), false, false).unwrap();
        assert!(!path.exists());
        set_enabled_in_home(home.path(), false, false).unwrap();
    }

    #[test]
    fn flatpak_entry_launches_the_application_through_flatpak() {
        let home = tempfile::tempdir().unwrap();
        set_enabled_in_home(home.path(), true, true).unwrap();

        let contents = fs::read_to_string(autostart_path_in_home(home.path())).unwrap();
        assert!(contents.contains("Exec=flatpak run com.chrisdaggas.datacleaner --background"));
    }
}
