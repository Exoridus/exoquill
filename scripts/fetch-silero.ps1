# Fetches the Silero VAD ONNX model and the ONNX Runtime DLL for the optional
# `silero` neural-VAD dictation feature. Both land under runtimes/, where dev.ps1
# points ExoQuill; like the other AI assets they are bundled as Tauri resources
# for release and are not in git.
#
#   pwsh scripts/fetch-silero.ps1
#
# Then build with the feature enabled, e.g.:
#   pnpm tauri build -- --features silero
#   cargo build -p exoquill-desktop --features silero
#
# The ONNX Runtime version must match the ABI `ort` was built against: the pinned
# ort 2.0.0-rc.10 loads onnxruntime 1.22.x and refuses any other version at
# runtime. Re-running is idempotent unless -Force is given.

param(
    [string]$OnnxRuntimeVersion = "1.22.0",
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$modelsDir = Join-Path $root "runtimes\models"
$ortDir = Join-Path $root "runtimes\onnxruntime"
New-Item -ItemType Directory -Force -Path $modelsDir | Out-Null
New-Item -ItemType Directory -Force -Path $ortDir | Out-Null

# 1. Silero VAD v5 model (MIT-licensed).
$modelPath = Join-Path $modelsDir "silero_vad.onnx"
if ($Force -or -not (Test-Path $modelPath)) {
    $modelUrl = "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx"
    Write-Host "Downloading silero_vad.onnx ..."
    Invoke-WebRequest -Uri $modelUrl -OutFile $modelPath
} else {
    Write-Host "Model already present: $modelPath"
}

# 2. ONNX Runtime DLL (Windows x64), extracted from the official release zip.
$dllPath = Join-Path $ortDir "onnxruntime.dll"
if ($Force -or -not (Test-Path $dllPath)) {
    $pkg = "onnxruntime-win-x64-$OnnxRuntimeVersion"
    $zipUrl = "https://github.com/microsoft/onnxruntime/releases/download/v$OnnxRuntimeVersion/$pkg.zip"
    $tmpZip = Join-Path $env:TEMP "$pkg.zip"
    $tmpDir = Join-Path $env:TEMP $pkg
    Write-Host "Downloading $pkg.zip ..."
    Invoke-WebRequest -Uri $zipUrl -OutFile $tmpZip
    Expand-Archive -Path $tmpZip -DestinationPath $tmpDir -Force
    Copy-Item -Path (Join-Path $tmpDir "$pkg\lib\onnxruntime.dll") -Destination $dllPath -Force
    Remove-Item $tmpZip -Force
    Remove-Item $tmpDir -Recurse -Force
} else {
    Write-Host "ONNX Runtime already present: $dllPath"
}

Write-Host "Done. Build dictation with the neural VAD via a --features silero build."
