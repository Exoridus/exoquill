# Sets up a local Python venv with Kokoro-82M for the EXPERIMENTAL Kokoro TTS
# sidecar (scripts/kokoro-server.py). The weights are Apache-2.0 licensed
# (commercial ok) and the model runs on CPU, so no CUDA GPU is required.
#
#   pwsh scripts/setup-kokoro.ps1
#
# Requires Python 3.12. After setup, start the sidecar and point ExoQuill at it
# via dev.ps1 (EXOQUILL_KOKORO_*). The model has a fixed set of built-in voices;
# no reference .wav clips are needed.

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$venv = Join-Path $root ".venv-kokoro"
$py = Join-Path $venv "Scripts\python.exe"

if (-not (Test-Path $py)) {
    Write-Host "Creating venv at $venv (Python 3.12) ..."
    if (Get-Command py -ErrorAction SilentlyContinue) {
        py -3.12 -m venv $venv
    } else {
        python -m venv $venv
    }
}

& $py -m pip install --upgrade pip wheel

# PyTorch (CPU is sufficient for Kokoro-82M).
& $py -m pip install "torch>=2.1" "torchaudio>=2.1" --index-url "https://download.pytorch.org/whl/cpu"

# Kokoro pip package.
& $py -m pip install kokoro soundfile

Write-Host ""
Write-Host "Done. Start the sidecar with:"
Write-Host "  $py scripts\kokoro-server.py --port 8023"
Write-Host "Or let ExoQuill auto-start it: the EXOQUILL_KOKORO_* lines in scripts\dev.ps1."
