//! Query matching and result ranking.

use std::collections::HashSet;

use crate::files::Hit;
use crate::history::History;
use crate::index::Item;

/// Maximum number of results kept after ranking.
pub const MAX_RESULTS: usize = 50;

/// Highest score a fuzzy (in-order letters) match can get.
const FUZZY_MAX: i64 = 300;

/// Scores how well `query` matches `name`. Both must already be lowercase.
/// Returns `None` when it doesn't match at all.
pub fn match_score(name: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let base = if name == query {
        1000
    } else if name.starts_with(query) {
        800
    } else if words(name).any(|w| w.starts_with(query)) {
        600
    } else if initials(name).starts_with(query) {
        550
    } else if name.contains(query) {
        400
    } else {
        subsequence_score(name, query)?
    };
    // Prefer shorter names among equally good matches.
    Some(base - name.chars().count().min(50) as i64)
}

fn words(name: &str) -> impl Iterator<Item = &str> {
    name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty())
}

/// "visual studio code" -> "vsc"
fn initials(name: &str) -> String {
    words(name).filter_map(|w| w.chars().next()).collect()
}

/// All query chars appear in order; tighter matches score higher.
fn subsequence_score(name: &str, query: &str) -> Option<i64> {
    let mut gaps = 0i64;
    let mut chars = name.chars();
    for q in query.chars() {
        loop {
            match chars.next() {
                Some(c) if c == q => break,
                Some(_) => gaps += 1,
                None => return None,
            }
        }
    }
    Some((FUZZY_MAX - gaps * 10).max(50))
}

/// Like `match_score`, but fuzzy matches don't count: over thousands of
/// file names they are mostly noise.
pub fn file_score(name: &str, query: &str) -> Option<i64> {
    match_score(name, query).filter(|&s| s > FUZZY_MAX)
}

/// Ranks programs and file hits together, best first. Launch history
/// (frecency) is added on top of the match quality, so frequently and
/// recently used programs win over programs you never open. Files lose their
/// tier penalty, so they show up below good program matches. An empty query
/// lists the most frecent programs only.
pub fn rank<'a>(items: &'a [Item], files: &'a [Hit], query: &str, history: &History, now: u64) -> Vec<&'a Item> {
    let query = query.trim().to_lowercase();
    let mut scored: Vec<(i64, &Item)> = items
        .iter()
        .filter_map(|item| {
            let frecency = history.score(&item.key, now);
            if query.is_empty() {
                return (frecency > 0).then_some((frecency, item));
            }
            let m = match_score(&item.name_lower, &query)?;
            Some((m + frecency, item))
        })
        .collect();
    // Desktop shortcuts are found both as programs and as files.
    let programs: HashSet<&str> = items.iter().map(|i| i.key.as_str()).collect();
    scored.extend(files.iter().filter(|(f, _)| !programs.contains(f.key.as_str())).filter_map(|(f, penalty)| {
        let m = file_score(&f.name_lower, &query)?;
        Some((m - penalty + history.score(&f.key, now), f))
    }));
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    scored.truncate(MAX_RESULTS);
    scored.into_iter().map(|(_, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Target;

    fn item(name: &str) -> Item {
        Item::new(name.to_string(), Target::Shortcut(format!(r"C:\{name}.lnk")))
    }

    fn names(items: Vec<&Item>) -> Vec<&str> {
        items.iter().map(|i| i.name.as_str()).collect()
    }

    #[test]
    fn match_tiers() {
        let exact = match_score("code", "code").unwrap();
        let prefix = match_score("codeblocks", "code").unwrap();
        let word = match_score("visual studio code", "code").unwrap();
        let acronym = match_score("visual studio code", "vsc").unwrap();
        let fuzzy = match_score("notepad", "ntp").unwrap();
        assert!(exact > prefix && prefix > word && word > acronym && acronym > fuzzy);
        assert_eq!(match_score("notepad", "xyz"), None);
    }

    #[test]
    fn history_outranks_slightly_better_match() {
        let items = vec![item("Calculator"), item("Visual Studio Code")];
        let mut history = History::empty();
        let now = 1_000_000;
        history.record(&items[1].key, now - 60);
        history.record(&items[1].key, now - 120);
        history.record(&items[1].key, now - 180);
        // "c" is a prefix of Calculator but only a word start in VS Code.
        assert_eq!(names(rank(&items, &[], "c", &history, now)), ["Visual Studio Code", "Calculator"]);
    }

    #[test]
    fn empty_query_lists_recent_only() {
        let items = vec![item("A"), item("B"), item("C")];
        let mut history = History::empty();
        history.record(&items[2].key, 10);
        assert_eq!(names(rank(&items, &[], "", &history, 20)), ["C"]);
    }

    #[test]
    fn files_rank_below_programs_and_skip_fuzzy() {
        let items = vec![item("Notepad")];
        let files = vec![(item("notes.txt"), 300), (item("nxoxtxe.txt"), 300), (item("Notepad"), 150)];
        let history = History::empty();
        // The Notepad file has the program's key, so it is deduplicated.
        assert_eq!(names(rank(&items, &files, "note", &history, 0)), ["Notepad", "notes.txt"]);
    }
}
