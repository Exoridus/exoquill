# Sets up a local Python venv with Resemble AI Chatterbox Multilingual for the
# EXPERIMENTAL Chatterbox TTS sidecar (scripts/chatterbox-server.py). The weights
# are MIT-licensed (commercial ok), but the model needs a CUDA GPU to be usable.
#
#   pwsh scripts/setup-chatterbox.ps1                 # CUDA wheels (default cu128)
#   pwsh scripts/setup-chatterbox.ps1 -Cuda cu124     # RTX 30xx/40xx
#
# Pick -Cuda to match your GPU: cu128 (default) covers Blackwell (RTX 50xx,
# sm_120) and is backward-compatible down to sm_70; cu124 for RTX 30xx/40xx.
# Requires Python 3.12. Then create a voices folder with one or more 10-30 s
# reference .wav clips (each file = one voice), start the sidecar, and point
# ExoQuill at it via dev.ps1 (EXOQUILL_CHATTERBOX_*).

param(
    [string]$Cuda = "cu128",
    [string]$Root = (Split-Path $PSScriptRoot -Parent)
)

$ErrorActionPreference = "Stop"
$venv = Join-Path $Root ".venv-chatterbox"
$py = Join-Path $venv "Scripts\python.exe"
$voices = Join-Path $Root "chatterbox-voices"

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

# Chatterbox Multilingual pip package.
& $py -m pip install chatterbox-tts

# A default voices folder so the sidecar has something to offer on first run.
if (-not (Test-Path $voices)) {
    New-Item -ItemType Directory -Path $voices | Out-Null
}

Write-Host ""
Write-Host "Done. Add one or more 10-30s reference .wav clips to:"
Write-Host "  $voices"
Write-Host "Then start the sidecar with:"
Write-Host "  $py scripts\chatterbox-server.py --port 8022 --voices $voices"
Write-Host "Or let ExoQuill auto-start it: the EXOQUILL_CHATTERBOX_* lines in scripts\dev.ps1."
