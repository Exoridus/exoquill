# Sets up a local Python venv with Zyphra Zonos-v0.1 for the EXPERIMENTAL Zonos
# TTS sidecar (scripts/zonos-server.py). Unlike XTTS, the Zonos weights are
# Apache-2.0 (fine to redistribute), but the model needs a CUDA GPU to be usable.
#
#   pwsh scripts/setup-zonos.ps1                 # CUDA wheels (default cu128)
#   pwsh scripts/setup-zonos.ps1 -Cuda cu124     # RTX 30xx/40xx
#
# Pick -Cuda to match your GPU: cu128 (default) covers Blackwell (RTX 50xx,
# sm_120) and is backward-compatible down to sm_70; cu124 for RTX 30xx/40xx if
# you prefer. cu128 needs torch >= 2.7 (sm_120 kernels landed in 2.7). Requires
# Python 3.12 and git. Then create a voices folder with one or more 10-30 s
# reference .wav clips (each file = one voice), start the sidecar, and point
# ExoQuill at it via dev.ps1 (EXOQUILL_ZONOS_*).
#
# Zonos uses eSpeak NG for phonemization; we install `espeakng-loader`, which
# bundles the shared library so no system-wide eSpeak NG install is needed. The
# first synthesis downloads the model weights into the Hugging Face cache.

param(
    [string]$Cuda = "cu128",
    [string]$Root = (Split-Path $PSScriptRoot -Parent)
)

$ErrorActionPreference = "Stop"
$venv = Join-Path $Root ".venv-zonos"
$py = Join-Path $venv "Scripts\python.exe"
$src = Join-Path $Root ".zonos-src"
$voices = Join-Path $Root "zonos-voices"

if (-not (Test-Path $py)) {
    Write-Host "Creating venv at $venv (Python 3.12) ..."
    if (Get-Command py -ErrorAction SilentlyContinue) {
        py -3.12 -m venv $venv
    } else {
        python -m venv $venv
    }
}

& $py -m pip install --upgrade pip wheel

# PyTorch (+ torchaudio, used to load reference clips) from the index matching
# your GPU. Pinned to 2.7/2.8: new enough for Blackwell sm_120 kernels (cu128),
# but below 2.9. Both from the same index to match ABIs.
& $py -m pip install "torch>=2.7,<2.9" "torchaudio>=2.7,<2.9" --index-url "https://download.pytorch.org/whl/$Cuda"

# Zonos from a git *clone* installed editable (-e). A plain `pip install git+...`
# builds a wheel that drops the `zonos/backbone` subpackage (ModuleNotFoundError:
# zonos.backbone at runtime); an editable install links the source tree directly,
# so all subpackages resolve. Transformer variant only — no mamba-ssm needed.
if (-not (Test-Path (Join-Path $src ".git"))) {
    Write-Host "Cloning Zonos into $src ..."
    git clone --depth 1 https://github.com/Zyphra/Zonos.git $src
}
& $py -m pip install -e $src
# Bundled eSpeak NG library so phonemizer works without a system install.
& $py -m pip install espeakng-loader numpy

# A default voices folder so the sidecar has something to offer on first run.
if (-not (Test-Path $voices)) {
    New-Item -ItemType Directory -Path $voices | Out-Null
}

Write-Host ""
Write-Host "Done. Add one or more 10-30s reference .wav clips to:"
Write-Host "  $voices"
Write-Host "Then start the sidecar with:"
Write-Host "  $py scripts\zonos-server.py --port 8021 --voices $voices"
Write-Host "Or let ExoQuill auto-start it: the EXOQUILL_ZONOS_* lines in scripts\dev.ps1."
