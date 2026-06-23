# Fetches the CPU build of ONNX Runtime (onnxruntime.dll) into runtimes/onnxruntime/,
# for the native ONNX features (Kokoro TTS, optional Silero VAD). Bundled as a Tauri
# resource for release; dev.ps1 / the providers resolve it via ORT_DYLIB_PATH.
#
#   pwsh scripts/fetch-onnxruntime.ps1
#
# The ABI must match the pinned `ort` 2.0.0-rc.10, which loads onnxruntime 1.22.x.
# Kokoro runs **CPU-only** (its istftnet `ConvTranspose` op fails at runtime under
# the DirectML EP — verified), and CPU is faster than real time for the 82M model,
# so the lean CPU build is all we ship. Re-running is idempotent unless -Force.

param(
    [string]$OnnxRuntimeVersion = "1.22.0",
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$ortDir = Join-Path $root "runtimes\onnxruntime"
New-Item -ItemType Directory -Force -Path $ortDir | Out-Null

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
    Write-Host "onnxruntime.dll already present: $dllPath"
}

Write-Host "Done -> $dllPath"
