param()

$ErrorActionPreference = 'Stop'

function Find-Executable {
  param(
    [string]$CommandName,
    [string[]]$CandidatePaths
  )

  $command = Get-Command $CommandName -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($command -and (Test-Path $command.Source -PathType Leaf)) {
    return $command.Source
  }

  foreach ($candidate in $CandidatePaths) {
    if ($candidate -and (Test-Path $candidate -PathType Leaf)) {
      return $candidate
    }
  }

  return $null
}

$glazeWmExe = Find-Executable -CommandName 'glazewm.exe' -CandidatePaths @(
  (Join-Path $env:ProgramFiles 'glzr.io\GlazeWM\cli\glazewm.exe'),
  (Join-Path $env:ProgramFiles 'glzr.io\GlazeWM\glazewm.exe')
)
$zebarExe = Find-Executable -CommandName 'zebar.exe' -CandidatePaths @(
  (Join-Path $env:ProgramFiles 'glzr.io\Zebar\zebar.exe')
)

if (-not $glazeWmExe) {
  throw 'GlazeWM was not found. Install it from https://github.com/glzr-io/glazewm/releases and rerun this script.'
}
if (-not $zebarExe) {
  throw 'Zebar was not found. Install it from https://github.com/glzr-io/zebar/releases and rerun this script.'
}

$configSource = Join-Path $PSScriptRoot 'glazewm\config.yaml'
$widgetSource = Join-Path $PSScriptRoot 'zebar\dotfiles-bar'
foreach ($source in @($configSource, (Join-Path $widgetSource 'zpack.json'),
    (Join-Path $widgetSource 'with-glazewm.html'),
    (Join-Path $widgetSource 'styles.css'))) {
  if (-not (Test-Path $source -PathType Leaf)) {
    throw "Required dotfiles source file is missing: $source"
  }
}

$glzrRoot = Join-Path $env:USERPROFILE '.glzr'
$glazeWmConfigDir = Join-Path $glzrRoot 'glazewm'
$zebarRoot = Join-Path $glzrRoot 'zebar'
$widgetTarget = Join-Path $zebarRoot 'dotfiles-bar'
$settingsTarget = Join-Path $zebarRoot 'settings.json'
$backupRoot = Join-Path $glzrRoot ("dotfiles-backup\" + (Get-Date -Format 'yyyyMMdd-HHmmss'))

New-Item -ItemType Directory -Force -Path $glazeWmConfigDir, $zebarRoot | Out-Null

$existingFiles = @(
  @{
    Source = Join-Path $glazeWmConfigDir 'config.yaml'
    Backup = Join-Path $backupRoot 'glazewm\config.yaml'
  },
  @{
    Source = $settingsTarget
    Backup = Join-Path $backupRoot 'zebar\settings.json'
  }
)
foreach ($file in $existingFiles) {
  if (Test-Path $file.Source -PathType Leaf) {
    New-Item -ItemType Directory -Force -Path (Split-Path $file.Backup) | Out-Null
    Copy-Item -LiteralPath $file.Source -Destination $file.Backup
  }
}

if (Test-Path $widgetTarget -PathType Container) {
  $widgetBackup = Join-Path $backupRoot 'zebar\dotfiles-bar'
  New-Item -ItemType Directory -Force -Path (Split-Path $widgetBackup) | Out-Null
  Copy-Item -LiteralPath $widgetTarget -Destination $widgetBackup -Recurse
}

Copy-Item -LiteralPath $configSource `
  -Destination (Join-Path $glazeWmConfigDir 'config.yaml') -Force
Copy-Item -LiteralPath $widgetSource -Destination $zebarRoot -Recurse -Force

$settings = [ordered]@{
  '$schema' = 'https://github.com/glzr-io/zebar/raw/v3.3.1/resources/settings-schema.json'
  startupConfigs = @(
    [ordered]@{
      pack = 'dotfiles-bar'
      widget = 'with-glazewm'
      preset = 'default'
    }
  )
}
$settingsJson = $settings | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText(
  $settingsTarget,
  $settingsJson,
  (New-Object System.Text.UTF8Encoding($false))
)

$startupDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Startup'
$shortcutPath = Join-Path $startupDir 'GlazeWM.lnk'
New-Item -ItemType Directory -Force -Path $startupDir | Out-Null
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $glazeWmExe
$shortcut.Arguments = 'start'
$shortcut.WorkingDirectory = Split-Path $glazeWmExe
$shortcut.Description = 'Start GlazeWM at sign-in'
$shortcut.Save()

Write-Output 'Installed GlazeWM and Zebar dotfiles for the current user.'
Write-Output "GlazeWM: $glazeWmExe"
Write-Output "Zebar: $zebarExe"
Write-Output "Startup shortcut: $shortcutPath"
if (Test-Path $backupRoot) {
  Write-Output "Previous settings backed up to: $backupRoot"
}
Write-Output 'Sign out and back in (or restart Windows) to verify automatic startup.'
