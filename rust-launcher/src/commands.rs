//! Built-in commands: keyword shortcuts such as `y` for YouTube.
//!
//! To add one, add a row to `COMMANDS` and its icon to `assets/commands/`
//! (any .ico file; `assets/commands/make-icons.ps1` draws the bundled ones).

use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, HICON, LR_DEFAULTCOLOR};

#[derive(Debug)]
pub struct Command {
    pub name: &'static str,
    /// Lowercase keywords; typing one exactly puts the command first.
    pub aliases: &'static [&'static str],
    pub action: Action,
    /// Contents of an .ico file.
    pub icon: &'static [u8],
}

#[derive(Debug)]
pub enum Action {
    /// Opens a URL, file or folder with its default program.
    Open(&'static str),
    /// Runs a command line in a new Windows Terminal window.
    Terminal(&'static str),
}

pub static COMMANDS: &[Command] = &[
    Command {
        name: "YouTube",
        aliases: &["y", "yt"],
        action: Action::Open("https://www.youtube.com"),
        icon: include_bytes!("../assets/commands/youtube.ico"),
    },
    Command {
        name: "Claude Code",
        aliases: &["cc"],
        action: Action::Terminal("claude"),
        icon: include_bytes!("../assets/commands/claude.ico"),
    },
];

impl Command {
    /// Stable identity used for launch history.
    pub fn key(&self) -> String {
        format!("cmd:{}", self.name.to_lowercase())
    }

    pub fn subtitle(&self) -> String {
        let what = match self.action {
            Action::Open(target) => target.to_string(),
            Action::Terminal(cmd) => format!("{cmd} in Terminal"),
        };
        format!("{} · {what}", self.aliases.join(", "))
    }

    /// The icon image closest to `size` pixels, scaled to `size`.
    pub fn icon(&self, size: i32) -> HICON {
        let Some(image) = ico_image(self.icon, size) else { return std::ptr::null_mut() };
        unsafe { CreateIconFromResourceEx(image.as_ptr(), image.len() as u32, 1, 0x0003_0000, size, size, LR_DEFAULTCOLOR) }
    }
}

/// Picks the smallest image of at least `size` pixels from an .ico file (the
/// largest if all are smaller). Returns its PNG or DIB bytes.
fn ico_image(ico: &[u8], size: i32) -> Option<&[u8]> {
    let u16_at = |i: usize| Some(u16::from_le_bytes(ico.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |i: usize| Some(u32::from_le_bytes(ico.get(i..i + 4)?.try_into().ok()?) as usize);
    const ENTRY: usize = 16;
    let count = u16_at(4)? as usize;
    let entries = (0..count).map(|n| 6 + n * ENTRY).filter(|&e| e + ENTRY <= ico.len());
    // A width byte of 0 means 256.
    let width = |e: usize| if ico[e] == 0 { 256 } else { ico[e] as i32 };
    let best = entries.min_by_key(|&e| {
        let w = width(e);
        if w >= size { w - size } else { 1000 + size - w }
    })?;
    let (len, offset) = (u32_at(best + 8)?, u32_at(best + 12)?);
    ico.get(offset..offset + len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_lowercase_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for c in COMMANDS {
            for a in c.aliases {
                assert_eq!(*a, a.to_lowercase());
                assert!(seen.insert(*a), "duplicate alias {a}");
            }
        }
    }

    #[test]
    fn icons_parse() {
        for c in COMMANDS {
            assert!(ico_image(c.icon, 32).is_some(), "{} icon", c.name);
        }
    }
}
