//! Finds launchable programs: Start Menu / Desktop shortcuts plus Store
//! (UWP) apps reported by `Get-StartApps`, plus files opened before.

use std::collections::HashSet;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const EXTENSIONS: &[&str] = &["lnk", "exe", "url", "appref-ms"];
const MAX_DEPTH: usize = 6;

#[derive(Clone, Debug)]
pub enum Target {
    /// A file opened through the shell (shortcut, exe, url).
    Shortcut(String),
    /// An AppUserModelID, launched through `shell:AppsFolder`.
    App(String),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub name: String,
    pub name_lower: String,
    /// Stable identity used for launch history.
    pub key: String,
    pub target: Target,
}

impl Item {
    pub fn new(name: String, target: Target) -> Self {
        let key = match &target {
            Target::Shortcut(p) => p.to_lowercase(),
            Target::App(id) => format!("app:{id}"),
        };
        Item { name_lower: name.to_lowercase(), name, key, target }
    }

    pub fn subtitle(&self) -> &str {
        match &self.target {
            Target::Shortcut(p) => p,
            Target::App(_) => "App",
        }
    }
}

fn roots() -> Vec<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let start_menu = Path::new(r"Microsoft\Windows\Start Menu\Programs");
    [
        env("APPDATA").map(|d| d.join(start_menu)),
        env("ProgramData").map(|d| d.join(start_menu)),
        env("USERPROFILE").map(|d| d.join("Desktop")),
        env("OneDrive").map(|d| d.join("Desktop")),
        env("PUBLIC").map(|d| d.join("Desktop")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<Item>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, out);
            }
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if !EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        if name.eq_ignore_ascii_case("desktop") || name.to_lowercase().contains("uninstall") {
            continue;
        }
        out.push(Item::new(name.to_string(), Target::Shortcut(path.to_string_lossy().into_owned())));
    }
}

/// Fast filesystem scan of shortcut folders.
pub fn scan_shortcuts() -> Vec<Item> {
    let mut items = Vec::new();
    for root in roots() {
        walk(&root, 0, &mut items);
    }
    dedupe(items)
}

/// Slower (~1s) query for Store apps such as Calculator or Settings.
pub fn scan_apps() -> Vec<Item> {
    let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
                  Get-StartApps | ForEach-Object { $_.Name + \"`t\" + $_.AppID }";
    let Ok(output) = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let items = text
        .lines()
        .filter_map(|line| {
            let (name, id) = line.trim_end().split_once('\t')?;
            let lower = name.to_lowercase();
            if name.is_empty() || id.is_empty() || lower.contains("uninstall") {
                return None;
            }
            Some(Item::new(name.to_string(), Target::App(id.to_string())))
        })
        .collect();
    dedupe(items)
}

fn dedupe(items: Vec<Item>) -> Vec<Item> {
    let mut seen = HashSet::new();
    items.into_iter().filter(|i| seen.insert(i.name_lower.clone())).collect()
}

/// Files and folders opened from the launcher before, so they show up among
/// the recent items. History keys are lowercase; the real casing comes from
/// the filesystem. Paths that no longer exist are dropped.
pub fn recent_files(keys: &[String], shortcuts: &[Item]) -> Vec<Item> {
    let known: HashSet<&str> = shortcuts.iter().map(|i| i.key.as_str()).collect();
    keys.iter()
        .filter(|k| !known.contains(k.as_str()))
        .filter_map(|key| {
            let real = fs::canonicalize(key).ok()?;
            let real = real.to_string_lossy();
            let real = real.strip_prefix(r"\\?\").unwrap_or(&real);
            // Keep the history key if canonicalizing changed more than case.
            let path = if real.to_lowercase() == *key { real.to_string() } else { key.clone() };
            let name = Path::new(&path).file_name()?.to_str()?.to_string();
            Some(Item::new(name, Target::Shortcut(path)))
        })
        .collect()
}

/// Shortcuts win over apps with the same name; recent files never collide
/// with either, since they exclude shortcut paths.
pub fn merge(shortcuts: &[Item], apps: &[Item], recent: &[Item]) -> Vec<Item> {
    let mut items = dedupe(shortcuts.iter().chain(apps).cloned().collect());
    items.extend(recent.iter().cloned());
    items
}
