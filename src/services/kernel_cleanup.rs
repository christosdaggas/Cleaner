//! Removal of superseded kernel packages.
//!
//! This deliberately never shells out to a blanket `autoremove`. Generic
//! autoremove takes out *every* package nothing currently depends on, which on
//! a real system routinely includes things the user installed as a dependency
//! and now uses directly — and it has no concept of "keep N kernels". Instead
//! each supported package manager is asked to list its installed kernels, the
//! list is filtered here, and only explicit package names are ever handed to a
//! privileged removal command.
//!
//! Two kernels are protected unconditionally, regardless of the keep count:
//!   * the running kernel (`uname -r`), so the machine stays bootable;
//!   * the newest installed kernel, which is what the next boot will use after
//!     an upgrade that has not been rebooted into yet.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The package manager that owns kernel packages on this system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelManager {
    /// Fedora, RHEL and derivatives.
    Dnf,
    /// Debian, Ubuntu and derivatives.
    Apt,
    /// Arch and derivatives — only ever one kernel per package, nothing to prune.
    Pacman,
    /// openSUSE.
    Zypper,
}

/// One installed kernel, grouped by its version rather than by package: a single
/// Fedora kernel is spread over `kernel`, `kernel-core`, `kernel-modules` and
/// friends, and they must be removed together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledKernel {
    /// Kernel version as the package manager reports it, e.g. `6.8.5-301.fc40.x86_64`.
    pub version: String,
    /// Every package belonging to this version, in removal order.
    pub packages: Vec<String>,
    /// True when this is the kernel currently booted.
    pub running: bool,
}

/// What a cleanup would do, for display before anything is executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelPlan {
    pub manager: KernelManager,
    /// Kernels that would be removed, oldest first.
    pub removable: Vec<InstalledKernel>,
    /// Kernels that stay, newest first: the running one, the newest one, and
    /// however many the keep count preserves.
    pub retained: Vec<InstalledKernel>,
    /// Why nothing can be removed, when `removable` is empty.
    pub note: Option<String>,
}

impl KernelPlan {
    /// Every package name that would be passed to the removal command.
    pub fn packages(&self) -> Vec<String> {
        self.removable
            .iter()
            .flat_map(|kernel| kernel.packages.iter().cloned())
            .collect()
    }

    /// A human-readable summary for the confirmation dialog.
    pub fn summary(&self) -> String {
        if let Some(note) = &self.note {
            return note.clone();
        }
        let mut lines = Vec::new();
        for kernel in &self.removable {
            lines.push(format!("• {} ({} packages)", kernel.version, kernel.packages.len()));
        }
        let kept: Vec<String> = self
            .retained
            .iter()
            .map(|k| {
                if k.running {
                    format!("{} (running)", k.version)
                } else {
                    k.version.clone()
                }
            })
            .collect();
        format!(
            "Remove {} kernel version(s):\n{}\n\nKeeping: {}",
            self.removable.len(),
            lines.join("\n"),
            kept.join(", ")
        )
    }
}

fn find_command(command: &str) -> Option<PathBuf> {
    ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
        .iter()
        .map(|dir| Path::new(dir).join(command))
        .find(|path| path.is_file())
}

/// Which package manager owns kernels here. `pacman` is checked before the
/// rpm-based managers because Arch derivatives sometimes ship `rpm` as a plain
/// utility without using it for package management.
pub fn detect_manager() -> Option<KernelManager> {
    if find_command("pacman").is_some() {
        return Some(KernelManager::Pacman);
    }
    if find_command("apt-get").is_some() && find_command("dpkg-query").is_some() {
        return Some(KernelManager::Apt);
    }
    if find_command("zypper").is_some() {
        return Some(KernelManager::Zypper);
    }
    if find_command("dnf").is_some() {
        return Some(KernelManager::Dnf);
    }
    None
}

/// The running kernel release, e.g. `6.8.5-301.fc40.x86_64`.
fn running_release() -> String {
    Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Compare two kernel version strings by their numeric components, so
/// `6.10.0` sorts above `6.9.0` instead of below it as a plain string compare
/// would. Non-numeric separators are ignored; trailing text breaks ties.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let nums = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse::<u64>().ok())
            .collect()
    };
    let (na, nb) = (nums(a), nums(b));
    for i in 0..na.len().max(nb.len()) {
        let va = na.get(i).copied().unwrap_or(0);
        let vb = nb.get(i).copied().unwrap_or(0);
        match va.cmp(&vb) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    a.cmp(b)
}

/// Split installed kernels into what to remove and what to keep.
///
/// `keep` counts kernel *versions* to preserve, newest first. The running
/// kernel and the newest kernel are always retained on top of that, so a keep
/// count of 1 on a machine booted into an older kernel still preserves two.
pub fn plan_removal(
    manager: KernelManager,
    mut kernels: Vec<InstalledKernel>,
    keep: u32,
) -> KernelPlan {
    // Newest first.
    kernels.sort_by(|a, b| compare_versions(&b.version, &a.version));

    if manager == KernelManager::Pacman {
        return KernelPlan {
            manager,
            removable: Vec::new(),
            retained: kernels,
            note: Some(
                "Arch-based systems install a single package per kernel series, so no \
                 superseded kernel packages accumulate. Nothing to remove."
                    .to_string(),
            ),
        };
    }

    let keep = keep.max(1) as usize;
    let mut removable = Vec::new();
    let mut retained = Vec::new();

    for (index, kernel) in kernels.into_iter().enumerate() {
        // index 0 is the newest kernel and is always protected, as is whichever
        // kernel is currently booted.
        let protected = index == 0 || kernel.running || index < keep;
        if protected {
            retained.push(kernel);
        } else {
            removable.push(kernel);
        }
    }

    // Oldest first, so removal output reads chronologically.
    removable.reverse();

    let note = if removable.is_empty() {
        Some(format!(
            "No superseded kernels to remove: {} installed, keeping {}.",
            retained.len(),
            keep
        ))
    } else {
        None
    };

    KernelPlan {
        manager,
        removable,
        retained,
        note,
    }
}

/// Parse `rpm -q --qf '%{NAME} %{VERSION}-%{RELEASE}.%{ARCH}\n'` output into
/// kernels grouped by version.
///
/// The caller feeds this the result of querying `installonlypkg(kernel)` and
/// `installonlypkg(kernel-module)`, which is rpm's own marker for "a package the
/// system keeps several versions of". Matching on a `kernel-` name prefix
/// instead pulls in packages that merely start with the word: on Fedora,
/// `kernel-srpm-macros` is a build-macro package with a completely unrelated
/// version, and prefix matching happily offered it up for removal.
pub fn parse_rpm_kernels(output: &str, running: &str) -> Vec<InstalledKernel> {
    let mut by_version: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(version)) = (parts.next(), parts.next()) else {
            continue;
        };
        // `kernel-headers` is shared rather than install-only and would break
        // builds if removed by version; rpm does not mark it installonly, but
        // guard anyway in case a fallback path supplies it.
        if name.contains("headers") || name.contains("firmware") || name.contains("macros") {
            continue;
        }
        by_version
            .entry(version.to_string())
            .or_default()
            .push(format!("{name}-{version}"));
    }

    by_version
        .into_iter()
        .map(|(version, mut packages)| {
            packages.sort();
            InstalledKernel {
                running: !running.is_empty() && version == running,
                version,
                packages,
            }
        })
        .collect()
}

/// Parse `dpkg-query -W -f='${Package}\t${Status}\n'` output into kernels
/// grouped by the release encoded in the package name.
pub fn parse_dpkg_kernels(output: &str, running: &str) -> Vec<InstalledKernel> {
    const KERNEL_PREFIXES: [&str; 3] = ["linux-image-", "linux-modules-", "linux-headers-"];

    let mut by_version: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in output.lines() {
        let mut parts = line.split('\t');
        let (Some(name), Some(status)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !status.contains("installed") || status.contains("not-installed") {
            continue;
        }
        let Some(prefix) = KERNEL_PREFIXES.iter().find(|p| name.starts_with(**p)) else {
            continue;
        };
        let release = &name[prefix.len()..];
        // Meta packages such as `linux-image-generic` or `linux-image-amd64`
        // carry no version and must never be removed: they are what pulls in
        // future kernels.
        if !release.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        // `linux-modules-extra-6.8.0-40-generic` and
        // `linux-image-6.8.0-40-generic` share release `6.8.0-40-generic`.
        by_version
            .entry(release.to_string())
            .or_default()
            .push(name.to_string());
    }

    by_version
        .into_iter()
        .map(|(version, mut packages)| {
            packages.sort();
            InstalledKernel {
                running: !running.is_empty() && version == running,
                version,
                packages,
            }
        })
        .collect()
}

/// Query the system for installed kernels and work out what can go.
pub fn build_plan(keep: u32) -> Result<KernelPlan, String> {
    let Some(manager) = detect_manager() else {
        return Err("No supported package manager was found on this system.".to_string());
    };
    let running = running_release();

    let kernels = match manager {
        KernelManager::Pacman => Vec::new(),
        KernelManager::Apt => {
            let out = run_query(
                "dpkg-query",
                &["-W", "-f=${Package}\\t${Status}\\n", "linux-image-*", "linux-modules-*", "linux-headers-*"],
            )?;
            parse_dpkg_kernels(&out, &running)
        }
        KernelManager::Dnf | KernelManager::Zypper => {
            // rpm's own notion of an install-only package, so nothing that
            // merely starts with "kernel-" can slip in.
            let mut out = String::new();
            for capability in ["installonlypkg(kernel)", "installonlypkg(kernel-module)"] {
                out.push_str(&run_query(
                    "rpm",
                    &[
                        "-q",
                        "--whatprovides",
                        capability,
                        "--qf",
                        "%{NAME} %{VERSION}-%{RELEASE}.%{ARCH}\n",
                    ],
                )?);
            }
            parse_rpm_kernels(&out, &running)
        }
    };

    Ok(plan_removal(manager, kernels, keep))
}

fn run_query(command: &str, args: &[&str]) -> Result<String, String> {
    let Some(path) = find_command(command) else {
        return Err(format!("The {command} executable was not found."));
    };
    let output = Command::new(path)
        .args(args)
        .output()
        .map_err(|error| format!("Could not run {command}: {error}"))?;
    // dpkg-query exits non-zero when a glob matches nothing, which is not an
    // error for us — an empty list simply means no kernels of that shape.
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The privileged command line that removes the planned packages. Returns
/// `None` when there is nothing to remove, so callers cannot accidentally run a
/// package manager with an empty package list (which several interpret as
/// "operate on everything").
pub fn removal_command(plan: &KernelPlan) -> Option<Vec<String>> {
    let packages = plan.packages();
    if packages.is_empty() {
        return None;
    }
    let mut args: Vec<String> = match plan.manager {
        KernelManager::Dnf => vec!["dnf".into(), "-y".into(), "remove".into()],
        KernelManager::Apt => vec!["apt-get".into(), "-y".into(), "purge".into()],
        KernelManager::Zypper => {
            vec!["zypper".into(), "--non-interactive".into(), "remove".into()]
        }
        KernelManager::Pacman => return None,
    };
    args.extend(packages);
    Some(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel(version: &str, running: bool) -> InstalledKernel {
        InstalledKernel {
            version: version.to_string(),
            packages: vec![format!("kernel-core-{version}")],
            running,
        }
    }

    #[test]
    fn versions_compare_numerically_not_lexically() {
        assert_eq!(compare_versions("6.10.0", "6.9.0"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions("6.8.0-40", "6.8.0-9"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions("6.8.5", "6.8.5"), std::cmp::Ordering::Equal);
    }

    #[test]
    fn running_kernel_is_never_removed() {
        let kernels = vec![
            kernel("6.10.0", false),
            kernel("6.9.0", true), // booted into an older kernel
            kernel("6.8.0", false),
            kernel("6.7.0", false),
        ];
        let plan = plan_removal(KernelManager::Dnf, kernels, 1);
        let removed: Vec<_> = plan.removable.iter().map(|k| k.version.as_str()).collect();
        assert!(!removed.contains(&"6.9.0"), "running kernel must be retained");
        assert!(!removed.contains(&"6.10.0"), "newest kernel must be retained");
        assert_eq!(removed, vec!["6.7.0", "6.8.0"], "oldest first");
    }

    #[test]
    fn keep_count_is_honoured() {
        let kernels = vec![
            kernel("6.10.0", true),
            kernel("6.9.0", false),
            kernel("6.8.0", false),
            kernel("6.7.0", false),
        ];
        let plan = plan_removal(KernelManager::Dnf, kernels, 3);
        assert_eq!(plan.retained.len(), 3);
        assert_eq!(plan.removable.len(), 1);
        assert_eq!(plan.removable[0].version, "6.7.0");
    }

    #[test]
    fn a_single_installed_kernel_is_left_alone() {
        let plan = plan_removal(KernelManager::Dnf, vec![kernel("6.10.0", true)], 1);
        assert!(plan.removable.is_empty());
        assert!(plan.note.is_some());
        assert!(removal_command(&plan).is_none());
    }

    #[test]
    fn arch_reports_nothing_to_do() {
        let plan = plan_removal(KernelManager::Pacman, vec![kernel("6.10.0", true)], 1);
        assert!(plan.removable.is_empty());
        assert!(plan.note.as_deref().unwrap().contains("Arch"));
        assert!(removal_command(&plan).is_none());
    }

    #[test]
    fn rpm_output_groups_packages_by_version() {
        let out = "\
kernel 6.9.0-1.fc40.x86_64
kernel-core 6.9.0-1.fc40.x86_64
kernel-modules 6.9.0-1.fc40.x86_64
kernel-core 6.8.0-1.fc40.x86_64
kernel-headers 6.9.0-1.fc40.x86_64
";
        let kernels = parse_rpm_kernels(out, "6.9.0-1.fc40.x86_64");
        assert_eq!(kernels.len(), 2);
        let running: Vec<_> = kernels.iter().filter(|k| k.running).collect();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].version, "6.9.0-1.fc40.x86_64");
        assert_eq!(running[0].packages.len(), 3, "headers must be excluded");
    }

    #[test]
    fn build_macro_packages_are_not_kernels() {
        // Regression: `kernel-srpm-macros` matched a `kernel-` prefix and was
        // offered for removal alongside real kernels, with a version
        // (1.0-28) that sorted as the "oldest kernel" on the machine.
        let out = "\
kernel-core 7.1.8-200.fc44.x86_64
kernel-srpm-macros 1.0-28.fc44.noarch
";
        let kernels = parse_rpm_kernels(out, "7.1.8-200.fc44.x86_64");
        assert_eq!(kernels.len(), 1);
        assert_eq!(kernels[0].version, "7.1.8-200.fc44.x86_64");
    }

    #[test]
    fn dpkg_meta_packages_are_never_listed() {
        let out = "\
linux-image-generic\tinstall ok installed
linux-image-6.8.0-40-generic\tinstall ok installed
linux-modules-6.8.0-40-generic\tinstall ok installed
linux-image-6.8.0-31-generic\tinstall ok installed
linux-image-6.8.0-20-generic\tdeinstall ok not-installed
";
        let kernels = parse_dpkg_kernels(out, "6.8.0-40-generic");
        let versions: Vec<_> = kernels.iter().map(|k| k.version.as_str()).collect();
        assert_eq!(versions, vec!["6.8.0-31-generic", "6.8.0-40-generic"]);
        assert!(
            !kernels.iter().any(|k| k.packages.iter().any(|p| p == "linux-image-generic")),
            "the meta package pulls in future kernels and must never be removed"
        );
        assert!(
            !kernels.iter().any(|k| k.version.contains("6.8.0-20")),
            "packages already removed must be ignored"
        );
    }

    #[test]
    fn removal_command_uses_explicit_names_never_autoremove() {
        let kernels = vec![kernel("6.10.0", true), kernel("6.9.0", false), kernel("6.8.0", false)];
        let plan = plan_removal(KernelManager::Dnf, kernels, 1);
        let args = removal_command(&plan).expect("something to remove");
        assert!(!args.iter().any(|a| a.contains("autoremove")));
        assert_eq!(args[0], "dnf");
        assert!(args.contains(&"kernel-core-6.8.0".to_string()));
    }

    #[test]
    fn apt_purges_by_name() {
        let kernels = vec![kernel("6.10.0", true), kernel("6.9.0", false), kernel("6.8.0", false)];
        let plan = plan_removal(KernelManager::Apt, kernels, 1);
        let args = removal_command(&plan).expect("something to remove");
        assert_eq!(&args[..3], &["apt-get", "-y", "purge"]);
        assert!(!args.iter().any(|a| a.contains("autoremove")));
    }
}
