# Back to a fresh-install state, for testing the installer and the first run:
# uninstall audioviz, sweep up anything the uninstaller leaves, and remove the
# app's data (settings, scenes, presets, visuals) so the next start is a first
# start (the welcome sheet, factory files written fresh).
#
#   scripts/reset-install.ps1 -WhatIf        # list what it would remove, touch nothing
#   scripts/reset-install.ps1                # back up the data, then remove everything
#   scripts/reset-install.ps1 -KeepData      # uninstall only; settings and files stay
#   scripts/reset-install.ps1 -NoBackup      # remove the data without a backup
#
# The backup is a zip of %APPDATA%\audioviz in tmp\reset-backups\ (gitignored);
# restore it by unzipping into %APPDATA%. Dev runs (AUDIOVIZ_DEFAULTS=1) never
# read or write any of this, so they are unaffected either way.
[CmdletBinding(SupportsShouldProcess)]
param(
    [switch]$KeepData,
    [switch]$NoBackup
)
$ErrorActionPreference = "Stop"
$root = (Resolve-Path "$PSScriptRoot\..").Path

# Where the installer (installer/audioviz.iss, per-user) and the app put things.
$appId = "{A7C3F1E2-5B64-4D8A-9E1F-2C0B7D3A4F10}_is1"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\audioviz"
$uninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$appId"
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\audioviz"
$desktop = Join-Path ([Environment]::GetFolderPath("Desktop")) "audioviz.lnk"
$dataDir = Join-Path $env:APPDATA "audioviz"

function Remove-Thing([string]$path, [string]$what) {
    if (Test-Path -LiteralPath $path) {
        if ($PSCmdlet.ShouldProcess($path, "remove $what")) {
            Remove-Item -LiteralPath $path -Recurse -Force
            Write-Host "  removed $what  $path"
        }
    }
}

# 1. Nothing may hold the files open.
$running = Get-Process -Name audioviz -ErrorAction SilentlyContinue
if ($running -and $PSCmdlet.ShouldProcess("audioviz.exe ($($running.Count) running)", "stop")) {
    $running | Stop-Process -Force
    Start-Sleep -Seconds 1
    Write-Host "  stopped audioviz"
}

# 2. The uninstaller, silently, if there is one (it removes the files, the
#    shortcuts, the Run value and its own registry entry).
$uninstaller = $null
if (Test-Path $uninstallKey) {
    $s = (Get-ItemProperty $uninstallKey).UninstallString
    if ($s) { $uninstaller = $s.Trim('"') }
}
if (-not $uninstaller) {
    $u = Join-Path $installDir "unins000.exe"
    if (Test-Path $u) { $uninstaller = $u }
}
if ($uninstaller -and (Test-Path $uninstaller)) {
    if ($PSCmdlet.ShouldProcess($uninstaller, "run the uninstaller silently")) {
        $p = Start-Process -FilePath $uninstaller -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART" -PassThru -Wait
        # Inno's uninstaller copies itself to %TEMP% and finishes from there: wait for it.
        for ($i = 0; $i -lt 30 -and (Test-Path (Join-Path $installDir "unins000.exe")); $i++) { Start-Sleep -Milliseconds 500 }
        Write-Host "  uninstalled (exit $($p.ExitCode))"
    }
} else {
    Write-Host "  no uninstaller found (not installed, or already removed)"
}

# 3. Whatever is left: an older or interrupted install, a hand-copied exe.
Remove-Thing $installDir "install folder"
Remove-Thing $startMenu "Start menu folder"
Remove-Thing $desktop "desktop shortcut"
if ((Get-ItemProperty $runKey -ErrorAction SilentlyContinue).audioviz -and $PSCmdlet.ShouldProcess("$runKey\audioviz", "remove start-at-sign-in value")) {
    Remove-ItemProperty -Path $runKey -Name audioviz
    Write-Host "  removed start-at-sign-in value"
}
if ((Test-Path $uninstallKey) -and $PSCmdlet.ShouldProcess($uninstallKey, "remove uninstall entry")) {
    Remove-Item -Path $uninstallKey -Recurse -Force
    Write-Host "  removed uninstall entry"
}

# 4. The app's data: settings.json, scenes, presets, visuals (and the old looks folder).
if ($KeepData) {
    Write-Host "  kept data  $dataDir"
} elseif (Test-Path -LiteralPath $dataDir) {
    if (-not $NoBackup -and $PSCmdlet.ShouldProcess($dataDir, "back up to tmp\reset-backups")) {
        $backups = Join-Path $root "tmp\reset-backups"
        New-Item -ItemType Directory -Force -Path $backups | Out-Null
        $zip = Join-Path $backups ("audioviz-data-{0}.zip" -f (Get-Date -Format "yyyyMMdd-HHmmss"))
        Compress-Archive -Path $dataDir -DestinationPath $zip
        Write-Host "  backed up data  $zip"
    }
    Remove-Thing $dataDir "data folder"
}

if ($WhatIfPreference) { Write-Host "Dry run: nothing was changed." } else { Write-Host "Clean: the next install starts fresh." }
