# Fetches a Whisper ggml model into runtimes/models/, where dev.ps1 points
# ExoQuill's dictation feature (decisions D5/D8). The model is backend-agnostic —
# the GPU/CPU runtime that consumes it is built by scripts/build-whisper.ps1.
# Like the other AI models it is bundled as a Tauri resource for release and is
# not in git.
#
#   pwsh scripts/fetch-whisper.ps1                          # large-v3-turbo-q5_0 (default)
#   pwsh scripts/fetch-whisper.ps1 -Model large-v3-turbo   # full f16 turbo (~1.6 GB)
#   pwsh scripts/fetch-whisper.ps1 -Model base             # small dev/test model
#
# Default is the quantized large-v3-turbo (q5_0, ~574 MB): near-full quality at a
# fraction of the size, and very fast on the GPU runtime. Re-running is
# idempotent: an existing file is kept unless -Force is given.

param(
    [ValidateSet(
        "large-v3-turbo-q5_0", "large-v3-turbo-q8_0", "large-v3-turbo",
        "tiny", "base", "small", "medium"
    )]
    [string]$Model = "large-v3-turbo-q5_0",
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$modelsDir = Join-Path $root "runtimes\models"
New-Item -ItemType Directory -Force -Path $modelsDir | Out-Null

$modelName = "ggml-$Model.bin"
$modelPath = Join-Path $modelsDir $modelName
if ($Force -or -not (Test-Path $modelPath)) {
    $modelUrl = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$modelName"
    Write-Host "Downloading $modelName ..."
    Invoke-WebRequest -Uri $modelUrl -OutFile $modelPath
} else {
    Write-Host "Model already present: $modelPath"
}

Write-Host "Done. Build the GPU runtime with scripts/build-whisper.ps1 so dictation uses it."
