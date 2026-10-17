# Package a release: release build, Inno Setup installer, portable zip, SHA256SUMS,
# rendered winget manifests, release notes. Same script locally and in CI.
#
#   scripts/release.ps1                      # everything, version from Cargo.toml
#   scripts/release.ps1 -SkipBuild           # reuse target/release/audioviz.exe
#   scripts/release.ps1 -SkipBuild -SkipInstaller   # re-hash + re-render after signing
#
# Refuses to build while audioviz.exe is running (the exe is locked); pass
# -StopRunning to close it first.
[CmdletBinding()]
param(
    [string]$Version,
    [string]$Publisher = "Gregory Douglas",
    [string]$PackageId = "GregoryDouglas.audioviz",
    [string]$RepoUrl = "https://github.com/gregodouglas/audioviz",
    [string]$OutDir = "",
    [switch]$SkipBuild,
    [switch]$SkipInstaller,
    [switch]$StopRunning
)
$ErrorActionPreference = "Stop"
# ($PSScriptRoot is empty inside param() defaults on Windows PowerShell 5.1, so resolve here.)
$root = (Resolve-Path "$PSScriptRoot\..").Path
Set-Location $root
if (-not $OutDir) { $OutDir = Join-Path $root "dist" }

if (-not $Version) {
    $Version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
}
Write-Host "audioviz $Version  publisher='$Publisher'  id=$PackageId"
# GetFullPath first: New-Item mis-resolves a ".." segment and lands in the drive root.
$OutDir = [IO.Path]::GetFullPath($OutDir)
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$exe = "$root\target\release\audioviz.exe"

# --- build ---
if (-not $SkipBuild) {
    $running = Get-Process -Name audioviz -ErrorAction SilentlyContinue
    if ($running) {
        if ($StopRunning) { $running | Stop-Process -Force; Start-Sleep -Seconds 1 }
        else { throw "audioviz.exe is running (pid $($running.Id -join ',')); close it or pass -StopRunning" }
    }
    cargo build --release -p audioviz --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
if (-not (Test-Path $exe)) { throw "missing $exe" }
$exeVersion = (Get-Item $exe).VersionInfo.ProductVersion
if ($exeVersion -and ($exeVersion -ne $Version)) { throw "exe version $exeVersion != $Version (rebuild)" }

# --- installer ---
$setup = "$OutDir\audioviz-$Version-setup.exe"
if (-not $SkipInstaller) {
    $iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
    if (-not $iscc) {
        $iscc = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe", "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    if (-not $iscc) { throw "ISCC.exe (Inno Setup 6) not found; winget install JRSoftware.InnoSetup" }
    & $iscc /Q "/DAppVersion=$Version" "/DPublisher=$Publisher" "/DRepoUrl=$RepoUrl" "/DSourceDir=$root\target\release" "/DOutputDir=$OutDir" "$root\installer\audioviz.iss"
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }
}
if (-not (Test-Path $setup)) { throw "missing $setup" }

# --- portable zip ---
$zip = "$OutDir\audioviz-$Version-portable-win64.zip"
$stage = "$OutDir\portable"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $exe $stage
Copy-Item "$root\LICENSE" "$stage\LICENSE.txt"
Copy-Item "$root\README.md", "$root\PRIVACY.md" $stage
Remove-Item $zip -ErrorAction SilentlyContinue
Compress-Archive -Path "$stage\*" -DestinationPath $zip -CompressionLevel Optimal
Remove-Item -Recurse -Force $stage

# --- checksums ---
$sums = @()
foreach ($f in @($setup, $zip)) {
    $h = (Get-FileHash -Algorithm SHA256 $f).Hash.ToLower()
    $sums += "$h  $(Split-Path -Leaf $f)"
}
$sumsText = ($sums -join "`n") + "`n"
[IO.File]::WriteAllText("$OutDir\SHA256SUMS.txt", $sumsText)
$setupHash = (Get-FileHash -Algorithm SHA256 $setup).Hash.ToUpper()

# --- winget manifests ---
$pkgPath = ($PackageId -split '\.') -join '\'
$manifestDir = "$OutDir\winget\manifests\$($PackageId.Substring(0,1).ToLower())\$pkgPath\$Version"
Remove-Item -Recurse -Force "$OutDir\winget" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
$tokens = @{
    "{{PACKAGE_ID}}" = $PackageId
    "{{VERSION}}"    = $Version
    "{{PUBLISHER}}"  = $Publisher
    "{{REPO_URL}}"   = $RepoUrl
    "{{URL}}"        = "$RepoUrl/releases/download/v$Version/audioviz-$Version-setup.exe"
    "{{SHA256}}"     = $setupHash
    "{{DATE}}"       = (Get-Date -Format "yyyy-MM-dd")
}
foreach ($t in Get-ChildItem "$root\winget\*.yaml") {
    $text = Get-Content $t -Raw
    foreach ($k in $tokens.Keys) { $text = $text.Replace($k, $tokens[$k]) }
    $name = $t.Name.Replace("PACKAGE_ID", $PackageId)
    [IO.File]::WriteAllText("$manifestDir\$name", $text)
}
$manifestZip = "$OutDir\audioviz-$Version-winget-manifests.zip"
Remove-Item $manifestZip -ErrorAction SilentlyContinue
Compress-Archive -Path "$OutDir\winget\manifests" -DestinationPath $manifestZip
if (Get-Command winget -ErrorAction SilentlyContinue) {
    winget validate --manifest $manifestDir
    if ($LASTEXITCODE -ne 0) { throw "winget manifest validation failed" }
} else {
    Write-Warning "winget not found; manifest not validated"
}

# --- release notes ---
$notes = Get-Content "$root\docs\release-notes-template.md" -Raw
$notes = $notes.Replace("{{VERSION}}", $Version).Replace("{{SHA256SUMS}}", $sumsText.TrimEnd()).Replace("{{PACKAGE_ID}}", $PackageId)
[IO.File]::WriteAllText("$OutDir\RELEASE_NOTES.md", $notes)

Write-Host ""
Get-ChildItem $OutDir -File | ForEach-Object { "{0,12:N0}  {1}" -f $_.Length, $_.Name }
Write-Host "winget manifests: $manifestDir"
