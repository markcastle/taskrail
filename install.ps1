# Installs or updates Task Rail on Windows from the latest GitHub Release.
#
#   irm https://taskrail.uk/install.ps1 | iex
#
# Downloads the installer, runs it quietly for the current user (no admin
# prompt), and starts the app. Set GITHUB_TOKEN if the repository is private.

$ErrorActionPreference = 'Stop'
$repo = 'markcastle/taskrail'

$headers = @{ 'User-Agent' = 'taskrail-install' }
if ($env:GITHUB_TOKEN) { $headers['Authorization'] = "Bearer $env:GITHUB_TOKEN" }

Write-Host 'Looking up the latest Task Rail release...'
try {
    $release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers $headers
} catch {
    throw "Could not read the latest release of $repo. If the repository is private, set GITHUB_TOKEN first. ($($_.Exception.Message))"
}

$asset = $release.assets | Where-Object { $_.name -like '*-setup.exe' } | Select-Object -First 1
if (-not $asset) { throw "Release $($release.tag_name) has no Windows installer." }

$tmp = Join-Path $env:TEMP "taskrail-$($release.tag_name)-setup.exe"
Write-Host "Downloading Task Rail $($release.tag_name) ($([math]::Round($asset.size / 1MB, 1)) MB)..."
if ($env:GITHUB_TOKEN) {
    # Private repositories only hand out assets through the API URL.
    $dl = $headers.Clone(); $dl['Accept'] = 'application/octet-stream'
    Invoke-WebRequest $asset.url -OutFile $tmp -Headers $dl
} else {
    Invoke-WebRequest $asset.browser_download_url -OutFile $tmp -Headers $headers
}
# The file came from the internet; without this Windows asks before running it.
Unblock-File $tmp

# A running copy would hold the files the installer wants to replace.
Get-Process -Name 'task-rail', 'Task Rail', 'taskrail' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

Write-Host 'Installing...'
$proc = Start-Process -FilePath $tmp -ArgumentList '/S' -Wait -PassThru
if ($proc.ExitCode -ne 0) { throw "The installer exited with code $($proc.ExitCode)." }
Remove-Item $tmp -ErrorAction SilentlyContinue

# The per-user install lands under Local AppData; start whatever it put there.
$installDir = Join-Path $env:LOCALAPPDATA 'Task Rail'
$exe = Get-ChildItem -Path $installDir -Filter '*.exe' -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notlike 'uninstall*' } | Select-Object -First 1
if ($exe) {
    Start-Process -FilePath $exe.FullName
    Write-Host "Task Rail $($release.tag_name) is installed and running. It lives in the system tray; Ctrl+Shift+Space adds a task."
} else {
    Write-Host "Task Rail $($release.tag_name) is installed. Start it from the Start menu."
}
