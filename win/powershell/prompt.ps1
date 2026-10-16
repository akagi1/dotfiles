# Lightweight Git prompt: up-arrow N unpushed, + staged, ! unstaged, ? untracked.
function Get-DotfilesGitPrompt {
    $ErrorActionPreference = 'Continue'
    if (-not (Get-Command git -CommandType Application -ErrorAction SilentlyContinue)) { return '' }

    $statusLines = @(git --no-optional-locks status --porcelain=v1 --branch --untracked-files=normal --ignore-submodules=dirty 2>$null)
    if ($LASTEXITCODE -ne 0 -or $statusLines.Count -eq 0) { return '' }

    $header = $statusLines[0]
    $ahead = ''
    if ($header -match 'ahead ([0-9]+)') { $ahead = ' ' + [char]0x2191 + $Matches[1] }
    $branch = $header -replace '^## ', '' -replace '\.\.\..*$', ''
    $branch = $branch -replace '^(No commits yet on |Initial commit on )', ''
    if ($branch -eq 'HEAD (no branch)') { $branch = 'detached' }

    $staged = ''; $unstaged = ''; $untracked = ''
    foreach ($line in ($statusLines | Select-Object -Skip 1)) {
        if ($line.StartsWith('??')) { $untracked = '?' }
        elseif ($line.Length -ge 2) {
            if ($line[0] -ne ' ') { $staged = '+' }
            if ($line[1] -ne ' ') { $unstaged = '!' }
        }
    }
    $flags = "$staged$unstaged$untracked"
    if ($flags) { $flags = ' ' + $flags }
    return " [$branch$ahead$flags]"
}

function global:prompt {
    $savedExitCode = $global:LASTEXITCODE
    try {
        $path = $ExecutionContext.SessionState.Path.CurrentLocation.Path
        if ($path -eq $HOME) { $path = '~' }
        elseif ($path.StartsWith($HOME + '\', [StringComparison]::OrdinalIgnoreCase)) {
            $path = '~' + $path.Substring($HOME.Length)
        }
        Write-Host $path -NoNewline -ForegroundColor Green
        if ($ExecutionContext.SessionState.Path.CurrentLocation.Provider.Name -eq 'FileSystem') {
            Write-Host (Get-DotfilesGitPrompt) -NoNewline -ForegroundColor Yellow
        }
        Write-Host (' ' + ('>' * ($nestedPromptLevel + 1))) -NoNewline -ForegroundColor Green
        return ' '
    }
    finally { $global:LASTEXITCODE = $savedExitCode }
}
