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

This folder documents the current GlazeWM + Zebar setup, with rust-launcher as
the manually installed `Alt+Space` app launcher. It contains the GlazeWM
configuration and a self-contained Zebar widget pack. Setup and updates are
done manually; there is no installer script.

## Programs and prerequisites

- **Windows 10 or 11.** The square-corner effect in the GlazeWM config is
  Windows 11-only; GlazeWM ignores that effect on versions that do not support
  it.
- **GlazeWM.** Install from the [official releases](https://github.com/glzr-io/glazewm/releases).
  The GlazeWM README also documents `winget install GlazeWM`. The GlazeWM
  installer can optionally install Zebar as well.
- **Zebar.** Install the Windows build from
  [official releases](https://github.com/glzr-io/zebar/releases) if it was not
  installed with GlazeWM.
- **rust-launcher.** Download `rust-launcher.exe` from its
  [GitHub releases](https://github.com/akagi1/rust-launcher/releases/latest).
  It is a single portable exe with no installer.
- **Microsoft Edge WebView2 Runtime.** Zebar uses WebView2. It is commonly
  already present on Windows; if Zebar does not start, install the Evergreen
  Runtime from [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/).

No Node.js, package manager, or separate theme engine is required. This widget
uses Zebar's buildless HTML/React template.

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

1. Install GlazeWM and Zebar as described above. Install rust-launcher as
   described in the rust-launcher section.
2. Back up any existing files before replacing them. Copy
   `win\glazewm\config.yaml` to
   `%USERPROFILE%\.glzr\glazewm\config.yaml`.
3. Copy the repository's `win\zebar\dotfiles-bar` folder to
   `%USERPROFILE%\.glzr\zebar\dotfiles-bar`, creating the parent folder if
   needed.
4. Create or update `%USERPROFILE%\.glzr\zebar\settings.json` with:

   ```json
   {
     "$schema": "https://github.com/glzr-io/zebar/raw/v3.3.1/resources/settings-schema.json",
     "startupConfigs": [
       {
         "pack": "dotfiles-bar",
         "widget": "with-glazewm",
         "preset": "default"
       }
     ]
   }
   ```

5. Register GlazeWM as a current-user startup app by adding a string value
   named `GlazeWM` to
   `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Set its data to
   `"C:\Program Files\glzr.io\GlazeWM\cli\glazewm.exe" start` (adjust the
   executable path if needed). It will appear in Task Manager's **Startup
   apps** list. GlazeWM's `startup_commands` launches Zebar, whose settings
   load the `with-glazewm` widget.
6. Make sure rust-launcher is registered in the same `Run` key as
   `RustLauncher` (step 3 of the rust-launcher section).

To update the setup, back up the installed files first, then repeat the copy
steps for any changed configuration or widget files. Reload GlazeWM after
updating its configuration.

## What is configured

- GlazeWM config: `%USERPROFILE%\.glzr\glazewm\config.yaml`
- Zebar settings: `%USERPROFILE%\.glzr\zebar\settings.json`
- Local Zebar pack: `%USERPROFILE%\.glzr\zebar\dotfiles-bar\`
- GlazeWM sign-in startup app:
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\GlazeWM`
- rust-launcher exe: `%LOCALAPPDATA%\Programs\rust-launcher\rust-launcher.exe`
- rust-launcher sign-in startup app:
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\RustLauncher`
- rust-launcher launch history: `%APPDATA%\rust-launcher\history.tsv`
- One tiled window overlaps the screen edges by 1px and uses a 39px top gap
  to cover the native frame's transparent edge below the 40px bar. Multiple
  windows use 5px inner/edge gaps, with a 40px top gap for the bar.
- Keybindings use Alt as the main modifier. `Alt+1` through `Alt+9` switch
  workspaces; `Alt+Shift+1` through `Alt+Shift+9` move a window to a workspace.
- The focused window has no configured border; other windows have a grey border.
- `Alt+Enter` opens Windows Terminal using its configured default profile.
- Window title bars, including the minimize, maximize, and close buttons, are
  hidden for focused and unfocused windows. Some applications may render
  incorrectly without a title bar.
  Supported Windows 11 windows use square corners.
- The top bar shows workspaces, tiling direction, network, memory, CPU,
  battery, and date/time. Temperature/weather is not included.

The `it-agent` ignore rule is preserved from the source machine because it
prevented auxiliary windows from affecting its layout. It is harmless when
that process is not installed. Remove that rule from `glazewm\config.yaml` if
you do not want it on another machine.

## Machine-specific Windows settings

Windows taskbar auto-hide is not part of GlazeWM or Zebar configuration. Enable
it separately under **Settings → Personalization → Taskbar → Taskbar
behaviors → Automatically hide the taskbar** (Windows 11), or the equivalent
Taskbar setting on Windows 10.

The bar's HTML loads React/Babel and its icon font from public CDNs, so the
first load and icon rendering require network access.

# Windows GlazeWM configuration

When changing `win/glazewm/config.yaml`, remember that GlazeWM reads the
installed config at `%USERPROFILE%\.glzr\glazewm\config.yaml`, not this
repository file. For requested GlazeWM config changes, apply them to the
installed config without asking: back up the installed file, copy the updated
repository config into its place, reload it with
`glazewm.exe command wm-reload-config` (use the installed CLI path if it is
not on `PATH`), and verify the installed config contains the intended change.
