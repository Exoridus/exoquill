# Sets up a local Python venv with Coqui XTTS-v2 for the EXPERIMENTAL XTTS TTS
# sidecar (scripts/xtts-server.py). Test-only: the XTTS-v2 weights are
# non-commercial (CPML) and must not ship in ExoQuill's GPL build. The library
# (the maintained `coqui-tts` fork) is MPL-2.0.
#
#   pwsh scripts/setup-xtts.ps1                 # CUDA wheels (default cu128)
#   pwsh scripts/setup-xtts.ps1 -Cuda cpu       # CPU-only torch (slow)
#
# Pick -Cuda to match your GPU: cu128 for Blackwell (RTX 50xx, sm_120) — older
# cu124 wheels lack sm_120 kernels and fail at inference; cu124/cu121 for 40xx
# and earlier. Requires Python 3.12 (coqui-tts has no 3.13 wheels yet). Then
# start the sidecar
# and point ExoQuill at it:
#   .\.venv-xtts\Scripts\python.exe scripts\xtts-server.py --port 8020
#   $env:EXOQUILL_XTTS_URL = "http://127.0.0.1:8020"; pnpm dev
#
# The first synthesis downloads the model (~1.8 GB) into the Coqui cache and
# accepts the CPML via COQUI_TOS_AGREED=1 (set by the server).

param(
    [string]$Cuda = "cu128"
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$venv = Join-Path $root ".venv-xtts"
$py = Join-Path $venv "Scripts\python.exe"

if (-not (Test-Path $py)) {
    Write-Host "Creating venv at $venv (Python 3.12) ..."
    # Prefer an explicit 3.12 via the py launcher; fall back to `python`.
    if (Get-Command py -ErrorAction SilentlyContinue) {
        py -3.12 -m venv $venv
    } else {
        python -m venv $venv
    }
}

& $py -m pip install --upgrade pip wheel

# PyTorch (+ torchaudio, required by XTTS) from the index matching your GPU
# (cpu | cu121 | cu124 | cu128). Both from the same index to match ABIs. Pinned
# to 2.7/2.8: new enough for Blackwell sm_120 kernels (added in 2.7), but below
# 2.9 — from 2.9 coqui-tts also demands torchcodec (fragile on Windows).
& $py -m pip install "torch>=2.7,<2.9" "torchaudio>=2.7,<2.9" --index-url "https://download.pytorch.org/whl/$Cuda"

# Coqui TTS — maintained fork (idiap), MPL-2.0; pulls in XTTS-v2 support + numpy.
& $py -m pip install coqui-tts numpy
# coqui-tts needs transformers>=4.57, but transformers 5.x dropped a symbol it
# imports (isin_mps_friendly). Pin to the last 4.x line, which has both.
& $py -m pip install "transformers>=4.57,<5"

Write-Host ""
Write-Host "Done. Start the sidecar with:"
Write-Host "  $py scripts\xtts-server.py --port 8020"
Write-Host "Then, in another shell:"
Write-Host '  $env:EXOQUILL_XTTS_URL = "http://127.0.0.1:8020"; pnpm dev'
