# Opens the lab (ADR-0003) on one jelly of a species, held still in its
# scene's colours, in a window to turn it round.
#   scripts/lab-jelly.ps1                     # the moon jelly
#   scripts/lab-jelly.ps1 -Species atolla
#   scripts/lab-jelly.ps1 -Species lion -Png  # PNGs and a sheet instead
# Any further arguments go to audioviz-lab (see audioviz-lab --help), e.g.
#   scripts/lab-jelly.ps1 -Species nettle --at 4.3 --passes all,glow
param(
    [ValidateSet("moon", "nettle", "crystal", "atolla", "comb", "lion")]
    [string]$Species = "moon",
    [switch]$Png
)
# (The rest arrive in $args: a [Parameter()] attribute would make this an
# advanced script, whose common parameters take "--out" for -OutVariable.)
$root = Split-Path $PSScriptRoot -Parent
$exe = "$root\target\release\audioviz-lab.exe"
if (-not (Test-Path $exe)) { throw "build it first: cargo build --release -p audioviz" }
# Where each species lives in the factory scenes (scene, layer).
$where = @{
    moon    = @("Jellyfish", 1)
    nettle  = @("Jellyfish", 0)
    lion    = @("Giants", 0)
    atolla  = @("Deep sea", 0)
    comb    = @("Deep sea", 1)
    crystal = @("Deep sea", 2)
}
$scene, $layer = $where[$Species]
$labArgs = @("jellyfish", "--scene", $scene, "--layer", $layer, "--set", "count=1", "--set", "plankton=0", "--set", "view=close-up", "--pan", "0,0.15", "--name", $Species)
if (-not $Png) { $labArgs += "--window" }
& $exe @labArgs @args
