# Fetches the native Kokoro TTS assets into runtimes/, plus its runtime deps (the
# CPU ONNX Runtime + a portable espeak-ng). For dev (dev.ps1 points here) and as the
# source a release bundles (tauri.kokoro.conf.json).
#
#   pwsh scripts/fetch-kokoro.ps1            # German (Martin) + deps  [default]
#   pwsh scripts/fetch-kokoro.ps1 -English   # also the English voices
#   pwsh scripts/fetch-kokoro.ps1 -SkipDeps  # models only (deps already fetched)
#
# German (Godelaune/Kokoro-82M-ONNX-German-Martin, Apache-2.0, single-speaker
# "Martin") is the out-of-the-box voice. English (onnx-community/Kokoro-82M-v1.0-
# ONNX, five voices) is optional. Both run natively over ONNX Runtime on the CPU
# (faster than real time) — no Python sidecar. Re-running is idempotent unless -Force.

param(
    [switch]$English,
    [switch]$SkipDeps,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$models = Join-Path $root "runtimes\models"

function Fetch-File($url, $path) {
    if (-not $Force -and (Test-Path $path)) {
        Write-Host "present: $path"
        return
    }
    New-Item -ItemType Directory -Force -Path (Split-Path $path) | Out-Null
    Write-Host "Downloading $(Split-Path $path -Leaf) ..."
    Invoke-WebRequest -Uri $url -OutFile $path
}

# German (Martin) — the OOTB voice.
$deBase = "https://huggingface.co/Godelaune/Kokoro-82M-ONNX-German-Martin/resolve/main"
Fetch-File "$deBase/kokoro-martin.onnx" (Join-Path $models "kokoro-de\kokoro-martin.onnx")
Fetch-File "$deBase/voices-martin.npz"  (Join-Path $models "kokoro-de\voices-martin.npz")

# English (optional).
if ($English) {
    $enBase = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main"
    Fetch-File "$enBase/onnx/model.onnx" (Join-Path $models "kokoro\model.onnx")
    Fetch-File "$enBase/tokenizer.json"  (Join-Path $models "kokoro\tokenizer.json")
    foreach ($v in "af_heart", "af_bella", "am_michael", "bf_emma", "bm_george") {
        Fetch-File "$enBase/voices/$v.bin" (Join-Path $models "kokoro\voices\$v.bin")
    }
}

# Runtime deps: CPU ONNX Runtime + portable espeak-ng. (Kokoro is CPU-only — its
# istftnet ConvTranspose op fails under DirectML; CPU is faster than real time.)
if (-not $SkipDeps) {
    $forward = @{}
    if ($Force) { $forward["Force"] = $true }
    & (Join-Path $PSScriptRoot "fetch-onnxruntime.ps1") @forward
    & (Join-Path $PSScriptRoot "fetch-espeak.ps1") @forward
}

Write-Host ""
Write-Host "Done. Bundle a release with:"
Write-Host "  pnpm tauri build --features kokoro --config src-tauri/tauri.kokoro.conf.json"
