# macOS desktop

- [Homebrew](https://brew.sh/): package manager.
- [yabai](https://github.com/asmvik/yabai/wiki): tiling window manager.
- [skhd](https://github.com/asmvik/skhd): Option/Alt shortcuts.
- [SketchyBar](https://felixkratz.github.io/SketchyBar/setup): status bar.

## Manual setup

1. Install the tools using their linked instructions.
2. Back up existing configs, then copy these folders:

   | Repository | Destination |
   | --- | --- |
   | `mac/yabai/` | `~/.config/yabai/` |
   | `mac/skhd/` | `~/.config/skhd/` |
   | `mac/sketchybar/` | `~/.config/sketchybar/` |

   For the zsh prompt, add `source /path/to/dotfiles/mac/zsh/prompt.zsh`
   to the end of `~/.zshrc` (using your checkout's absolute path). It shows
   the Git branch and `+` staged, `!` unstaged, `?` untracked changes with
   one Git status call per prompt; it skips submodule worktree scans.

3. Grant yabai and skhd **Accessibility** access in System Settings.
4. Create nine desktops; disable automatic Space rearrangement. Enable
   **Control+1–9** in Keyboard → Keyboard Shortcuts → Mission Control.
5. Enable menu bar/Dock auto-hide; disable Stage Manager.
6. Start login services:

   ```sh
   yabai --start-service
   skhd --start-service
   brew services start sketchybar
   ```

## Dotfiles

- BSP tiling, 10pt gaps, 30pt dark bar; built-in Menlo font.
- Option + H/J/K/L or arrows: focus; add Shift to move tiles.
- Option + 1–9: switch desktop; add Shift to move a window and follow.
- Moving windows between desktops on Sequoia requires the optional
  [SIP changes](https://github.com/asmvik/yabai/wiki/Disabling-System-Integrity-Protection)
  and [scripting addition](https://github.com/asmvik/yabai/wiki/Installing-yabai-(latest-release)#configure-scripting-addition).
- All shortcuts: [skhdrc](skhd/skhdrc).
- After copying updates: `yabai --restart-service`, `skhd --reload`,
  `sketchybar --reload`.
- Keep machine-specific settings and secrets outside tracked files.
