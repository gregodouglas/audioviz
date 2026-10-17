# Dev tool: launch audioviz for N seconds, grab its window via PrintWindow (works with
# flip-model swapchains where plain screen capture shows black), save PNG + log under target/.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/snapshot.ps1 [-Seconds 8] [-Exe path]
param(
    [string]$Exe = "W:\projects\audioviz\target\debug\audioviz.exe",
    [string]$Shot = "$PSScriptRoot\shot2.png",
    [string]$Log = "$PSScriptRoot\run2.log",
    [int]$Seconds = 8
)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Win {
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
Add-Type -AssemblyName System.Drawing
$env:RUST_LOG = "info"
$p = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $Log -WorkingDirectory "W:\projects\audioviz"
Start-Sleep -Seconds $Seconds
$p.Refresh()
$h = $p.MainWindowHandle
Write-Output "hwnd=$h title='$($p.MainWindowTitle)'"
if ($h -ne [IntPtr]::Zero) {
    $r = New-Object Win+RECT
    [Win]::GetWindowRect($h, [ref]$r) | Out-Null
    $w = $r.R - $r.L; $hh = $r.B - $r.T
    Write-Output "window ${w}x${hh}"
    $bmp = New-Object System.Drawing.Bitmap $w, $hh
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    $ok = [Win]::PrintWindow($h, $hdc, 2)  # PW_RENDERFULLCONTENT
    $g.ReleaseHdc($hdc)
    Write-Output "printwindow=$ok"
    $bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
}
if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
Get-Content $Log | Select-String -NotMatch "VK_LAYER|objects: \(type|Loader Message|cooperative"
