# Sets up a local Python venv with Alibaba Qwen3-TTS for the EXPERIMENTAL Qwen3
# TTS sidecar (scripts/qwen3tts-server.py). Weights are Apache-2.0 (commercial
# ok), but the model needs a CUDA GPU to be usable.
#
#   pwsh scripts/setup-qwen3tts.ps1                 # CUDA wheels (default cu128)
#   pwsh scripts/setup-qwen3tts.ps1 -Cuda cu124     # RTX 30xx/40xx
#
# -Root  : where to create .venv-qwen3 + qwen3-voices (release passes the writable
#          app-data sidecars dir; defaults to the repo root for dev).
# -Model : HF model id (default 1.7B CustomVoice; predefined speakers + cloning).
# Requires Python 3.12.

param(
    [string]$Cuda = "cu128",
    [string]$Root = (Split-Path $PSScriptRoot -Parent),
    [string]$Model = "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice"
)

$ErrorActionPreference = "Stop"
$venv = Join-Path $Root ".venv-qwen3"
$py = Join-Path $venv "Scripts\python.exe"
$voices = Join-Path $Root "qwen3-voices"

if (-not (Test-Path $py)) {
    Write-Host "Creating venv at $venv (Python 3.12) ..."
    if (Get-Command py -ErrorAction SilentlyContinue) {
        py -3.12 -m venv $venv
    } else {
        python -m venv $venv
    }
}

& $py -m pip install --upgrade pip wheel

# PyTorch from the index matching your GPU.
& $py -m pip install "torch>=2.7,<2.9" "torchaudio>=2.7,<2.9" --index-url "https://download.pytorch.org/whl/$Cuda"

# Qwen3-TTS pip package.
& $py -m pip install -U qwen-tts

# flash-attn is optional — the server falls back to sdpa/eager when it's missing.
& $py -m pip install flash-attn --no-build-isolation
if ($LASTEXITCODE -ne 0) {
    Write-Host "flash-attn übersprungen (optional; der Server nutzt dann sdpa/eager)."
    $global:LASTEXITCODE = 0
}

# A default voices folder so cloning has somewhere to look.
if (-not (Test-Path $voices)) {
    New-Item -ItemType Directory -Path $voices | Out-Null
}

Write-Host ""
Write-Host "Done. Built-in speakers work out of the box (Vivian, Serena, ...)."
Write-Host "For voice cloning, add <name>.wav AND <name>.txt (its transcript) to:"
Write-Host "  $voices"
Write-Host "Then start the sidecar with:"
Write-Host "  $py scripts\qwen3tts-server.py --port 8023 --voices $voices --model $Model"
Write-Host "Or let ExoQuill auto-start it (EXOQUILL_QWEN3_* in scripts\dev.ps1)."
