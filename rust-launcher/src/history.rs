//! Launch history and "frecency" (frequency + recency) scoring.
//!
//! Each launch target keeps the timestamps of its most recent launches. Its
//! score is the sum of a weight per launch, where recent launches weigh more,
//! so apps you use often *and* lately float to the top.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// How many launches per target are remembered.
const MAX_STAMPS: usize = 10;

const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;

pub struct History {
    path: Option<PathBuf>,
    stamps: HashMap<String, Vec<u64>>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Weight of a single launch that happened `age` seconds ago.
fn weight(age: u64) -> i64 {
    match age {
        a if a < 4 * HOUR => 100,
        a if a < DAY => 70,
        a if a < 3 * DAY => 50,
        a if a < 7 * DAY => 30,
        a if a < 30 * DAY => 15,
        _ => 5,
    }
}

impl History {
    pub fn empty() -> Self {
        History { path: None, stamps: HashMap::new() }
    }

    /// Loads history from `%APPDATA%\rust-launcher\history.tsv`.
    pub fn load() -> Self {
        let path = std::env::var_os("APPDATA")
            .map(|d| PathBuf::from(d).join("rust-launcher").join("history.tsv"));
        let mut history = History { path, stamps: HashMap::new() };
        if let Some(text) = history.path.as_ref().and_then(|p| fs::read_to_string(p).ok()) {
            history.parse(&text);
        }
        history
    }

    fn parse(&mut self, text: &str) {
        for line in text.lines() {
            let Some((key, stamps)) = line.split_once('\t') else { continue };
            let stamps: Vec<u64> = stamps.split(' ').filter_map(|s| s.parse().ok()).collect();
            if !stamps.is_empty() {
                self.stamps.insert(key.to_string(), stamps);
            }
        }
    }

    fn serialize(&self) -> String {
        let mut out = String::new();
        for (key, stamps) in &self.stamps {
            let stamps: Vec<String> = stamps.iter().map(u64::to_string).collect();
            out.push_str(key);
            out.push('\t');
            out.push_str(&stamps.join(" "));
            out.push('\n');
        }
        out
    }

    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, self.serialize());
    }

    pub fn record(&mut self, key: &str, at: u64) {
        let stamps = self.stamps.entry(key.to_string()).or_default();
        stamps.push(at);
        if stamps.len() > MAX_STAMPS {
            stamps.remove(0);
        }
    }

    /// Keys of launched paths: everything but Store apps.
    pub fn path_keys(&self) -> Vec<String> {
        self.stamps.keys().filter(|k| !k.starts_with("app:")).cloned().collect()
    }

    pub fn score(&self, key: &str, now: u64) -> i64 {
        self.stamps
            .get(key)
            .map(|s| s.iter().map(|&t| weight(now.saturating_sub(t))).sum())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_and_frequent_scores_higher() {
        let mut h = History::empty();
        let now = 100 * DAY;
        h.record("old", now - 20 * DAY);
        h.record("recent", now - HOUR);
        h.record("frequent", now - 2 * DAY);
        h.record("frequent", now - 2 * DAY);
        h.record("frequent", now - 2 * DAY);
        assert!(h.score("recent", now) > h.score("old", now));
        assert!(h.score("frequent", now) > h.score("recent", now));
        assert_eq!(h.score("never", now), 0);
    }

    #[test]
    fn keeps_only_latest_stamps() {
        let mut h = History::empty();
        for i in 0..25 {
            h.record("a", i);
        }
        assert_eq!(h.stamps["a"].len(), MAX_STAMPS);
        assert_eq!(h.stamps["a"][0], 15);
    }

    #[test]
    fn roundtrips_through_text() {
        let mut h = History::empty();
        h.record(r"C:\x\App.lnk", 5);
        h.record(r"C:\x\App.lnk", 7);
        let mut h2 = History::empty();
        h2.parse(&h.serialize());
        assert_eq!(h2.stamps[r"C:\x\App.lnk"], vec![5, 7]);
    }
}
