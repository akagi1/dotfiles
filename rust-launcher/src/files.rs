//! On-demand file search without an index: every query starts a fresh
//! breadth-first walk over a priority list of folders, which is abandoned as
//! soon as the query changes. The Desktop is always searched completely, then
//! the user profile, then the other fixed drives until the time budget runs
//! out. Shallow files are found first, and the OS directory cache makes
//! repeated walks fast.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::mem;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows_sys::Win32::Storage::FileSystem::{
    GetDriveTypeW, GetLogicalDrives, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM,
};

use crate::index::{Item, Target};
use crate::matcher;

/// Shorter queries match too much to be worth a disk walk.
pub const MIN_QUERY: usize = 2;
/// Stop walking once this many files matched.
const MAX_HITS: usize = 300;
/// Walking time for everything after the Desktop.
const BUDGET: Duration = Duration::from_secs(5);
/// How often partial results are sent while walking.
const FLUSH_EVERY: Duration = Duration::from_millis(100);

const DRIVE_FIXED: u32 = 3;

/// Skipped wherever they appear: caches and dependency trees.
const SKIP_DIRS: &[&str] = &["appdata", "node_modules", "__pycache__", "site-packages"];
/// Skipped at a drive root: system folders, and `Users`, which the profile
/// tier already covered.
const SKIP_ROOT_DIRS: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "users",
    "recovery",
    "perflogs",
    "msocache",
];

/// A matching file plus its tier penalty, which ranks it below programs and
/// deeper tiers below the Desktop.
pub type Hit = (Item, i64);

struct Root {
    path: PathBuf,
    penalty: i64,
    /// Drive roots skip `SKIP_ROOT_DIRS`.
    drive: bool,
    /// Desktops ignore `BUDGET`.
    budgeted: bool,
}

fn roots() -> Vec<Root> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let root = |path, penalty, drive, budgeted| Root { path, penalty, drive, budgeted };
    let mut roots: Vec<Root> = [env("USERPROFILE"), env("OneDrive"), env("PUBLIC")]
        .into_iter()
        .flatten()
        .map(|d| root(d.join("Desktop"), 150, false, false))
        .collect();
    roots.extend(env("USERPROFILE").map(|d| root(d, 300, false, true)));
    let drives = unsafe { GetLogicalDrives() };
    for letter in 0..26u8 {
        if drives & (1 << letter) == 0 {
            continue;
        }
        let path = format!("{}:\\", (b'A' + letter) as char);
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        if unsafe { GetDriveTypeW(wide.as_ptr()) } == DRIVE_FIXED {
            roots.push(root(PathBuf::from(path), 450, true, true));
        }
    }
    roots
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// Walks all roots, calling `emit` with batches of hits for `query`, until
/// done, `MAX_HITS` is reached, the budget runs out or `alive` returns false.
/// A (possibly empty) batch is emitted after each root, so the first one
/// arrives as soon as the Desktop is searched.
pub fn search(query: &str, alive: impl Fn() -> bool, mut emit: impl FnMut(Vec<Hit>)) {
    let query = query.trim().to_lowercase();
    let start = Instant::now();
    let mut searched: HashSet<String> = HashSet::new();
    let mut hits = Vec::new();
    let mut total = 0;
    let mut flushed = Instant::now();

    for root in roots() {
        let mut queue = VecDeque::from([root.path.clone()]);
        while let Some(dir) = queue.pop_front() {
            if !alive() || total >= MAX_HITS || (root.budgeted && start.elapsed() > BUDGET) {
                emit(hits);
                return;
            }
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            let at_drive_root = root.drive && dir == root.path;
            for entry in entries.flatten() {
                // On Windows this comes from the directory listing: no extra I/O.
                let Ok(meta) = entry.metadata() else { continue };
                if meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0 {
                    continue;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if name.starts_with(['.', '$']) {
                    continue;
                }
                let lower = name.to_lowercase();
                let path = entry.path();
                // Junctions and symlinks are not dirs here, so loops are impossible.
                if meta.is_dir()
                    && !SKIP_DIRS.contains(&lower.as_str())
                    && !(at_drive_root && SKIP_ROOT_DIRS.contains(&lower.as_str()))
                    && !searched.contains(&path_key(&path))
                {
                    queue.push_back(path.clone());
                }
                if matcher::file_score(&lower, &query).is_some() {
                    let item = Item::new(name.to_string(), Target::Shortcut(path.to_string_lossy().into_owned()));
                    hits.push((item, root.penalty));
                    total += 1;
                }
            }
            if !hits.is_empty() && flushed.elapsed() > FLUSH_EVERY {
                emit(mem::take(&mut hits));
                flushed = Instant::now();
            }
        }
        emit(mem::take(&mut hits));
        flushed = Instant::now();
        searched.insert(path_key(&root.path));
    }
}

