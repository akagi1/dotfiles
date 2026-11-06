# Personal dotfiles

Platform-specific setup and configuration live in separate folders:

- [`win/`](win/AGENTS.md): GlazeWM, rustbar and rust-launcher on Windows.
- [`mac/`](mac/AGENTS.md): yabai, skhd and SketchyBar on macOS.
- `nix/`: placeholder for Linux/Nix dotfiles.
- `rustbar/`, `rust-launcher/`: Rust + Win32 top bar and launcher, built
  during Windows setup (see [`win/`](win/AGENTS.md)).
- [`skills/`](skills/AGENTS.md): Claude Code skills, symlinked into
  `~/.claude/skills`.

The platform folders can be versioned together in one repository. Keep
machine-specific settings and secrets out of tracked files.

