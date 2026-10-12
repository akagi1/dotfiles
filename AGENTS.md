# Windows GlazeWM configuration

When changing `win/glazewm/config.yaml`, remember that GlazeWM reads the
installed config at `%USERPROFILE%\.glzr\glazewm\config.yaml`, not this
repository file. Back up the installed config, copy the updated repository
config into that location, then reload it with
`glazewm.exe command wm-reload-config` (use the installed CLI path if it is
not on `PATH`). Verify the installed config contains the intended change.

Do not assume `Alt+Shift+R` will apply repository edits: that shortcut only
reloads the currently installed config.
