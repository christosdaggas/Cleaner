use gio::prelude::*;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageNodeKind {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct StorageNode {
    pub path: PathBuf,
    pub kind: StorageNodeKind,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub device: u64,
    pub inode: u64,
    pub children: Vec<StorageNode>,
}

impl StorageNode {
    pub fn display_name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn is_directory(&self) -> bool {
        self.kind == StorageNodeKind::Directory
    }

    pub fn find(&self, path: &Path) -> Option<&StorageNode> {
        if self.path == path {
            return Some(self);
        }
        if !path.starts_with(&self.path) {
            return None;
        }
        self.children.iter().find_map(|child| child.find(path))
    }
}

#[derive(Debug, Clone)]
pub struct StorageAnalysis {
    pub root: StorageNode,
    pub files_scanned: usize,
    pub directories_scanned: usize,
    pub skipped: Vec<String>,
}

impl StorageAnalysis {
    pub fn item_count(&self) -> usize {
        self.files_scanned + self.directories_scanned
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StorageScanOptions {
    pub minimum_size: u64,
    pub stay_on_filesystem: bool,
    pub max_children_per_directory: usize,
}

impl Default for StorageScanOptions {
    fn default() -> Self {
        Self {
            minimum_size: 100 * 1024 * 1024,
            stay_on_filesystem: true,
            max_children_per_directory: 500,
        }
    }
}

#[derive(Debug, Error)]
pub enum StorageAnalysisError {
    #[error("The storage scan was cancelled")]
    Cancelled,
    #[error("The selected location is not a local directory: {0}")]
    InvalidRoot(PathBuf),
    #[error("Could not inspect {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Default)]
struct DirectoryAccumulator {
    size: u64,
    children: Vec<StorageNode>,
}

pub fn analyze_storage(
    root: &Path,
    options: StorageScanOptions,
    cancelled: Arc<AtomicBool>,
) -> Result<StorageAnalysis, StorageAnalysisError> {
    let root = fs::canonicalize(root).map_err(|source| StorageAnalysisError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let root_metadata = fs::symlink_metadata(&root).map_err(|source| StorageAnalysisError::Io {
        path: root.clone(),
        source,
    })?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(StorageAnalysisError::InvalidRoot(root));
    }

    let mut accumulators: HashMap<PathBuf, DirectoryAccumulator> = HashMap::new();
    let mut hard_links: HashSet<(u64, u64)> = HashSet::new();
    let mut skipped = Vec::new();
    let mut files_scanned = 0usize;
    let mut directories_scanned = 0usize;
    let mut root_node = None;

    let walker = WalkDir::new(&root)
        .follow_links(false)
        .same_file_system(options.stay_on_filesystem)
        .contents_first(true)
        .max_open(32);

    for entry in walker {
        if cancelled.load(Ordering::Relaxed) {
            return Err(StorageAnalysisError::Cancelled);
        }

        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                if skipped.len() < 100 {
                    skipped.push(error.to_string());
                }
                continue;
            }
        };
        let path = entry.path();
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                if skipped.len() < 100 {
                    skipped.push(format!("{}: {error}", path.display()));
                }
                continue;
            }
        };

        if metadata.file_type().is_symlink() {
            continue;
        }

        if metadata.is_file() {
            let identity = (metadata.dev(), metadata.ino());
            if metadata.nlink() > 1 && !hard_links.insert(identity) {
                continue;
            }

            files_scanned += 1;
            let size = allocated_size(&metadata);
            let Some(parent) = path.parent() else {
                continue;
            };
            let accumulator = accumulators.entry(parent.to_path_buf()).or_default();
            accumulator.size = accumulator.size.saturating_add(size);
            if size >= options.minimum_size {
                accumulator.children.push(StorageNode {
                    path: path.to_path_buf(),
                    kind: StorageNodeKind::File,
                    size,
                    modified: metadata.modified().ok(),
                    device: metadata.dev(),
                    inode: metadata.ino(),
                    children: Vec::new(),
                });
            }
            continue;
        }

        if !metadata.is_dir() {
            continue;
        }

        directories_scanned += 1;
        let mut accumulator = accumulators.remove(path).unwrap_or_default();
        accumulator.size = accumulator.size.saturating_add(allocated_size(&metadata));
        accumulator
            .children
            .sort_by_key(|child| Reverse(child.size));
        accumulator
            .children
            .truncate(options.max_children_per_directory.max(1));

        let node = StorageNode {
            path: path.to_path_buf(),
            kind: StorageNodeKind::Directory,
            size: accumulator.size,
            modified: metadata.modified().ok(),
            device: metadata.dev(),
            inode: metadata.ino(),
            children: accumulator.children,
        };

        if path == root {
            root_node = Some(node);
            continue;
        }

        let Some(parent) = path.parent() else {
            continue;
        };
        let parent_accumulator = accumulators.entry(parent.to_path_buf()).or_default();
        parent_accumulator.size = parent_accumulator.size.saturating_add(node.size);
        if node.size >= options.minimum_size {
            parent_accumulator.children.push(node);
        }
    }

    let root = root_node.ok_or_else(|| StorageAnalysisError::InvalidRoot(root.clone()))?;
    Ok(StorageAnalysis {
        root,
        files_scanned,
        directories_scanned,
        skipped,
    })
}

fn allocated_size(metadata: &fs::Metadata) -> u64 {
    let allocated = metadata.blocks().saturating_mul(512);
    if allocated > 0 {
        allocated
    } else {
        metadata.len()
    }
}

#[derive(Debug, Clone)]
pub struct TrashTarget {
    pub path: PathBuf,
    pub kind: StorageNodeKind,
    pub size: u64,
    pub device: u64,
    pub inode: u64,
}

impl From<&StorageNode> for TrashTarget {
    fn from(node: &StorageNode) -> Self {
        Self {
            path: node.path.clone(),
            kind: node.kind,
            size: node.size,
            device: node.device,
            inode: node.inode,
        }
    }
}

#[derive(Debug, Default)]
pub struct TrashResult {
    pub moved: Vec<(PathBuf, u64)>,
    pub failed: Vec<(PathBuf, String)>,
}

impl TrashResult {
    pub fn bytes_moved(&self) -> u64 {
        self.moved.iter().map(|(_, size)| *size).sum()
    }
}

pub fn move_to_trash(scan_root: &Path, targets: &[TrashTarget]) -> TrashResult {
    let mut result = TrashResult::default();
    let targets = normalize_targets(targets);

    for target in targets {
        if let Err(reason) = validate_trash_target(scan_root, &target) {
            result.failed.push((target.path.clone(), reason));
            continue;
        }

        let file = gio::File::for_path(&target.path);
        match file.trash(None::<&gio::Cancellable>) {
            Ok(()) => result.moved.push((target.path, target.size)),
            Err(error) => result.failed.push((target.path, error.to_string())),
        }
    }

    result
}

fn normalize_targets(targets: &[TrashTarget]) -> Vec<TrashTarget> {
    let mut targets = targets.to_vec();
    targets.sort_by(|left, right| {
        left.path
            .components()
            .count()
            .cmp(&right.path.components().count())
            .then_with(|| left.path.cmp(&right.path))
    });

    let mut normalized: Vec<TrashTarget> = Vec::new();
    for target in targets {
        let covered_by_parent = normalized.iter().any(|parent| {
            parent.kind == StorageNodeKind::Directory
                && target.path != parent.path
                && target.path.starts_with(&parent.path)
        });
        if !covered_by_parent && !normalized.iter().any(|item| item.path == target.path) {
            normalized.push(target);
        }
    }
    normalized
}

fn validate_trash_target(scan_root: &Path, target: &TrashTarget) -> Result<(), String> {
    if target.path == scan_root || !target.path.starts_with(scan_root) {
        return Err("The scan root itself and paths outside it are protected".to_string());
    }
    if target.path == Path::new("/") {
        return Err("The filesystem root is protected".to_string());
    }
    if let Some(home) = dirs::home_dir() {
        if target.path == home || home.starts_with(&target.path) {
            return Err("The home directory and its ancestors are protected".to_string());
        }
    }

    let canonical_root = fs::canonicalize(scan_root)
        .map_err(|error| format!("Could not re-check the analyzed folder: {error}"))?;
    if canonical_root != scan_root {
        return Err("The analyzed folder changed after the scan; scan again first".to_string());
    }

    let canonical_target = fs::canonicalize(&target.path)
        .map_err(|error| format!("Could not re-check the selected item: {error}"))?;
    if canonical_target != target.path || !canonical_target.starts_with(&canonical_root) {
        return Err("The selected path changed after the scan; scan again first".to_string());
    }

    let metadata = fs::symlink_metadata(&target.path)
        .map_err(|error| format!("Could not re-check the selected item: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("Symbolic links cannot be moved by the analyzer".to_string());
    }
    if metadata.uid() != unsafe { libc::getuid() } {
        return Err("The selected item is not owned by the current user".to_string());
    }
    if metadata.dev() != target.device || metadata.ino() != target.inode {
        return Err("The selected item changed after the scan; scan again first".to_string());
    }
    if metadata.is_dir() != (target.kind == StorageNodeKind::Directory) {
        return Err("The selected item type changed after the scan".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn scan_aggregates_directories_and_filters_small_files() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        fs::create_dir(&nested).unwrap();
        let large = nested.join("large.bin");
        let small = nested.join("small.bin");
        fs::File::create(&large)
            .unwrap()
            .write_all(&vec![0_u8; 16 * 1024])
            .unwrap();
        fs::write(&small, b"small").unwrap();

        let analysis = analyze_storage(
            directory.path(),
            StorageScanOptions {
                minimum_size: 8 * 1024,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();

        let nested_node = analysis.root.find(&nested).unwrap();
        assert!(nested_node.size >= 16 * 1024);
        assert!(nested_node.find(&large).is_some());
        assert!(nested_node.find(&small).is_none());
        assert_eq!(analysis.files_scanned, 2);
    }

    #[test]
    fn scan_ignores_symlinks_and_duplicate_hard_links() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.bin");
        let hard_link = directory.path().join("hard-link.bin");
        let symlink = directory.path().join("symlink.bin");
        fs::write(&original, vec![1_u8; 8192]).unwrap();
        fs::hard_link(&original, &hard_link).unwrap();
        std::os::unix::fs::symlink(&original, &symlink).unwrap();

        let analysis = analyze_storage(
            directory.path(),
            StorageScanOptions {
                minimum_size: 1,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();

        assert_eq!(analysis.files_scanned, 1);
        assert_eq!(analysis.root.children.len(), 1);
        assert!(
            analysis.root.children[0].path == original
                || analysis.root.children[0].path == hard_link
        );
    }

    #[test]
    fn cancellation_stops_before_scanning() {
        let directory = tempfile::tempdir().unwrap();
        let cancelled = Arc::new(AtomicBool::new(true));
        let error = analyze_storage(directory.path(), StorageScanOptions::default(), cancelled)
            .unwrap_err();
        assert!(matches!(error, StorageAnalysisError::Cancelled));
    }

    #[test]
    fn target_normalization_removes_descendants_of_selected_folders() {
        let parent = TrashTarget {
            path: PathBuf::from("/tmp/analyzer/parent"),
            kind: StorageNodeKind::Directory,
            size: 100,
            device: 1,
            inode: 1,
        };
        let child = TrashTarget {
            path: parent.path.join("child.bin"),
            kind: StorageNodeKind::File,
            size: 50,
            device: 1,
            inode: 2,
        };

        let normalized = normalize_targets(&[child, parent.clone()]);
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].path, parent.path);
    }

    #[test]
    fn trash_validation_rejects_the_scan_root() {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        let metadata = fs::metadata(&root).unwrap();
        let target = TrashTarget {
            path: root.clone(),
            kind: StorageNodeKind::Directory,
            size: allocated_size(&metadata),
            device: metadata.dev(),
            inode: metadata.ino(),
        };

        assert!(validate_trash_target(&root, &target).is_err());
    }

    #[test]
    fn trash_validation_rejects_changed_items() {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        let file = root.join("large.bin");
        fs::write(&file, b"original").unwrap();
        let metadata = fs::metadata(&file).unwrap();
        let target = TrashTarget {
            path: file.clone(),
            kind: StorageNodeKind::File,
            size: allocated_size(&metadata),
            device: metadata.dev(),
            inode: metadata.ino(),
        };

        fs::rename(&file, root.join("original.bin")).unwrap();
        fs::write(&file, b"replacement").unwrap();

        assert!(validate_trash_target(&root, &target).is_err());
    }
}
