# Windows desktop setup

This folder reproduces the current GlazeWM + Zebar setup. It contains the
GlazeWM configuration, a self-contained Zebar widget pack, and a PowerShell
installer that copies the files into the current user's profile and enables
GlazeWM at sign-in.

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
- **Microsoft Edge WebView2 Runtime.** Zebar uses WebView2. It is commonly
  already present on Windows; if Zebar does not start, install the Evergreen
  Runtime from [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/).
- **PowerShell and Windows Script Host.** The setup script uses PowerShell and
  creates a Startup-folder shortcut via Windows Script Host.

No Node.js, package manager, or separate theme engine is required. This widget
uses Zebar's buildless HTML/React template.

## Install and enable startup

1. Install GlazeWM and Zebar as described above.
2. Open PowerShell in the cloned dotfiles folder and run:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File .\win\install.ps1
   ```

   The script checks that both programs are installed, copies the GlazeWM
   config, installs the local Zebar pack and its startup settings, and creates
   a `GlazeWM.lnk` shortcut in the current user's Windows Startup folder.
   Existing managed config files are backed up under
   `%USERPROFILE%\.glzr\dotfiles-backup\<timestamp>` before replacement.

3. Sign out and back in (or restart Windows). The Startup shortcut runs
   GlazeWM; GlazeWM's `startup_commands` launches Zebar; Zebar's settings
   launch the `with-glazewm` widget. No separate manual launch command is
   needed after setup.

To update the setup later, pull the dotfiles changes and run the same script
again. It backs up existing files before installing the updated versions.

## What is configured

- GlazeWM config: `%USERPROFILE%\.glzr\glazewm\config.yaml`
- Zebar settings: `%USERPROFILE%\.glzr\zebar\settings.json`
- Local Zebar pack: `%USERPROFILE%\.glzr\zebar\dotfiles-bar\`
- GlazeWM sign-in shortcut:
  `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\GlazeWM.lnk`
- One tiled window uses a 40px top gap and no other outer gaps. Multiple
  windows use 10px inner/edge gaps, with a 40px top gap for the bar.
- Keybindings use Alt as the main modifier. `Alt+1` through `Alt+9` switch
  workspaces; `Alt+Shift+1` through `Alt+Shift+9` move a window to a workspace.
- The focused-window glow is disabled; other windows keep their border.
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

