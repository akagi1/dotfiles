# Windows desktop setup

## PowerShell prompt

Dot-source `win/powershell/prompt.ps1` from your current-user PowerShell
profile (`$PROFILE.CurrentUserAllHosts`) using your checkout's absolute path:

```powershell
. 'C:\path\to\dotfiles\win\powershell\prompt.ps1'
```

Create the profile's parent directory and file if needed. Back up an existing
profile before editing it. For PowerShell 7, add the same line to its separate
current-user profile too. Open a new shell, or dot-source the profile to reload.
The prompt matches `mac/zsh/prompt.zsh`: green path, yellow Git branch,
`↑N` commits ahead of upstream, and `+` staged, `!` unstaged, `?` untracked
changes. It needs only Git, uses one status call, and skips submodule worktree
scans. Keep the checkout at that path, or update the profile's source line.

This folder documents the current GlazeWM setup, with rustbar as the top bar
and rust-launcher as the `Alt+Space` app launcher, both manually installed. It
contains the GlazeWM configuration. Setup and updates are done manually; there
is no installer script.

## Programs and prerequisites

- **Windows 10 or 11.** The square-corner effect in the GlazeWM config is
  Windows 11-only; GlazeWM ignores that effect on versions that do not support
  it.
- **GlazeWM.** Install from the [official releases](https://github.com/glzr-io/glazewm/releases).
  The GlazeWM README also documents `winget install GlazeWM`. Skip the
  optional bar it offers to install; rustbar is the bar.
- **rustbar.** Download `rustbar.exe` from its
  [GitHub releases](https://github.com/akagi1/rustbar/releases/latest).
  It is a single portable exe with no installer.
- **rust-launcher.** Download `rust-launcher.exe` from its
  [GitHub releases](https://github.com/akagi1/rust-launcher/releases/latest).
  It is a single portable exe with no installer.

## rustbar

rustbar draws a 28px bar along the top of every monitor: GlazeWM workspaces on
the left (click one to focus it), the date and time in the middle, and the
Wi-Fi network, CPU, GPU, memory and battery on the right, refreshed every
5 s. It talks to GlazeWM over its IPC socket and reconnects on its own, so it
does not need to start before or after GlazeWM.

1. Create the folder `%LOCALAPPDATA%\Programs\rustbar` and move the downloaded
   `rustbar.exe` into it. Autostart points at this path, so keep the exe there.
2. Run `rustbar.exe` once. The bar appears at the top of each monitor.
3. Right-click the bar and click **Start with Windows** so it shows a check
   mark. This registers it as a current-user startup app: a string value named
   `RustBar` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` with the
   quoted exe path as the data. You can also add that value by hand instead.

To update, right-click the bar and click **Exit**, replace the exe in the same
folder, and run it again.

## rust-launcher

rust-launcher opens with `Alt+Space` and ranks recently and frequently launched
programs first. GlazeWM does not bind this shortcut; rust-launcher registers
and handles it itself. The hotkey is fixed, so close any other launcher that
uses `Alt+Space` (Flow Launcher, PowerToys Run) or rust-launcher will show an
error and exit.

1. Create the folder `%LOCALAPPDATA%\Programs\rust-launcher` and move the
   downloaded `rust-launcher.exe` into it. Autostart points at this path, so
   keep the exe there.
2. Run `rust-launcher.exe` once. An arrow icon appears in the notification
   area (it may be in the hidden-icons overflow).
3. Right-click the arrow icon and click **Start with Windows** so it shows a
   check mark. This registers it as a current-user startup app: a string value
   named `RustLauncher` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
   with the quoted exe path as the data. It will appear in Task Manager's
   **Startup apps** list. You can also add that value by hand instead.
4. Press `Alt+Space`, type part of a program name, and press Enter.

To update, right-click the tray icon and click **Exit**, replace the exe in the
same folder, and run it again. Autostart keeps working because the path is
unchanged. If you previously used Flow Launcher, uninstall it and delete its
`Flow Launcher` value from the same `Run` key so the two do not compete for
`Alt+Space`.

## Manual setup

1. Install GlazeWM as described above. Install rustbar and rust-launcher as
   described in their sections.
2. Back up any existing file before replacing it. Copy
   `win\glazewm\config.yaml` to
   `%USERPROFILE%\.glzr\glazewm\config.yaml`.
3. Register GlazeWM as a current-user startup app by adding a string value
   named `GlazeWM` to
   `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Set its data to
   `"C:\Program Files\glzr.io\GlazeWM\cli\glazewm.exe" start` (adjust the
   executable path if needed). It will appear in Task Manager's **Startup
   apps** list.
4. Make sure rustbar and rust-launcher are registered in the same `Run` key as
   `RustBar` and `RustLauncher` (step 3 of their sections).

To update the setup, back up the installed config first, then copy the changed
config again and reload GlazeWM.

## What is configured

- GlazeWM config: `%USERPROFILE%\.glzr\glazewm\config.yaml`
- GlazeWM sign-in startup app:
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\GlazeWM`
- rustbar exe: `%LOCALAPPDATA%\Programs\rustbar\rustbar.exe`
- rustbar sign-in startup app:
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\RustBar`
- rust-launcher exe: `%LOCALAPPDATA%\Programs\rust-launcher\rust-launcher.exe`
- rust-launcher sign-in startup app:
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\RustLauncher`
- rust-launcher launch history: `%APPDATA%\rust-launcher\history.tsv`
- One tiled window overlaps the screen edges by 1px and uses a 27px top gap
  to cover the native frame's transparent edge below the 28px bar. Multiple
  windows use 5px inner/edge gaps, with a 28px top gap for the bar.
- Keybindings use Alt as the main modifier. `Alt+1` through `Alt+9` switch
  workspaces; `Alt+Shift+1` through `Alt+Shift+9` move a window to a workspace.
- The focused window has no configured border; other windows have a grey border.
- `Alt+Enter` opens Windows Terminal using its configured default profile.
- Window title bars, including the minimize, maximize, and close buttons, are
  hidden for focused and unfocused windows. Some applications may render
  incorrectly without a title bar.
  Supported Windows 11 windows use square corners.
- The top bar shows workspaces, date/time, Wi-Fi network, CPU, GPU, memory
  and battery.

The `it-agent` ignore rule is preserved from the source machine because it
prevented auxiliary windows from affecting its layout. It is harmless when
that process is not installed. Remove that rule from `glazewm\config.yaml` if
you do not want it on another machine.

## Debloating

Keep sign-in startup lean: only GlazeWM, rustbar and rust-launcher need to
start with Windows. Review **Task Manager → Startup apps** after installing
anything, and disable entries that are not needed at sign-in.

Disable rather than delete when an app re-adds its `Run` value on update.
Disabling is what Task Manager does: it keeps the `Run` value and adds a
12-byte binary value with the same name under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`,
starting with `03` (`02` re-enables it). The same in PowerShell:

```powershell
$k = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'
Set-ItemProperty $k -Name '<Run value name>' -Type Binary -Value ([byte[]](3,0,0,0,0,0,0,0,0,0,0,0))
```

Disabled on this setup:

- `BingWallpaperApp` and `BingSvc`: Bing Wallpaper and its background
  helper. Not needed for a static wallpaper.

Removed:

- Zebar: uninstalled, and removed from GlazeWM's `startup_commands`;
  replaced by rustbar.
- Flow Launcher: its `Flow.Launcher` `Run` value is deleted so it cannot take
  `Alt+Space` from rust-launcher; replaced by rust-launcher.

## Machine-specific Windows settings

Windows taskbar auto-hide is not part of GlazeWM or rustbar configuration.
Enable it separately under **Settings → Personalization → Taskbar → Taskbar
behaviors → Automatically hide the taskbar** (Windows 11), or the equivalent
Taskbar setting on Windows 10.

# Windows GlazeWM configuration

When changing `win/glazewm/config.yaml`, remember that GlazeWM reads the
installed config at `%USERPROFILE%\.glzr\glazewm\config.yaml`, not this
repository file. For requested GlazeWM config changes, apply them to the
installed config without asking: back up the installed file, copy the updated
repository config into its place, reload it with
`glazewm.exe command wm-reload-config` (use the installed CLI path if it is
not on `PATH`), and verify the installed config contains the intended change.
