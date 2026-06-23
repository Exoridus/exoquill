# Fetches a portable espeak-ng (binary + data) for the native Kokoro TTS G2P, into
# runtimes/espeak-ng/ (espeak-ng.exe + espeak-ng-data/). Bundled as a Tauri
# resource for release; dev.ps1 points EXOQUILL_ESPEAK / EXOQUILL_ESPEAK_DATA here.
#
#   pwsh scripts/fetch-espeak.ps1
#
# espeak-ng ships only an MSI for Windows, so we do an *administrative* extract
# (`msiexec /a`) — no install, no admin rights, no registry changes — to pull the
# files out into a portable folder. Re-running is idempotent unless -Force is given.

param(
    [string]$Version = "1.52.0",
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$dest = Join-Path $root "runtimes\espeak-ng"
$exe = Join-Path $dest "espeak-ng.exe"
if (-not $Force -and (Test-Path $exe)) {
    Write-Host "espeak-ng already present: $exe"
    return
}

New-Item -ItemType Directory -Force -Path $dest | Out-Null
$msi = Join-Path $env:TEMP "espeak-ng.msi"
$url = "https://github.com/espeak-ng/espeak-ng/releases/download/$Version/espeak-ng.msi"
Write-Host "Downloading espeak-ng $Version ..."
Invoke-WebRequest -Uri $url -OutFile $msi

$extract = Join-Path $env:TEMP "espeak-ng-admin"
if (Test-Path $extract) { Remove-Item $extract -Recurse -Force }
New-Item -ItemType Directory -Force -Path $extract | Out-Null
Write-Host "Extracting (administrative install) ..."
$p = Start-Process msiexec -ArgumentList @("/a", "`"$msi`"", "/qn", "TARGETDIR=`"$extract`"") -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "msiexec admin extract failed ($($p.ExitCode))" }

# The MSI lays files under "<extract>\eSpeak NG\". Flatten that into the dest so the
# exe and espeak-ng-data sit side by side (espeak-ng resolves data via --path).
$src = Get-ChildItem -Path $extract -Recurse -Filter "espeak-ng.exe" | Select-Object -First 1
if (-not $src) { throw "espeak-ng.exe not found after extract" }
Copy-Item -Path (Join-Path $src.Directory.FullName "*") -Destination $dest -Recurse -Force
Remove-Item $msi -Force
Remove-Item $extract -Recurse -Force

if (-not (Test-Path (Join-Path $dest "espeak-ng-data"))) {
    Write-Warning "espeak-ng-data not found next to the exe - German G2P may fail."
}
Write-Host "Done -> $dest"
