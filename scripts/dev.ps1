# Dev launcher: points ExoQuill at the local AI runtimes under runtimes/ and
# starts `pnpm tauri dev`. For release these runtimes are bundled as Tauri
# resources instead (see docs/decisions.md, D5).

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$runtimes = Join-Path $root "runtimes"

$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:EXOQUILL_TESSDATA = Join-Path $runtimes "tessdata"
$env:EXOQUILL_LLAMA = Join-Path $runtimes "llama\llama-completion.exe"
$env:EXOQUILL_FORMATTER_MODEL = Join-Path $runtimes "models\qwen2.5-1.5b-instruct-q4_k_m.gguf"
# Where the in-app model manager downloads/looks for on-demand models (dev: the
# same runtimes/ tree the providers resolve from).
$env:EXOQUILL_MODELS_ROOT = $runtimes
$env:EXOQUILL_PIPER = Join-Path $runtimes "piper\piper.exe"
$env:EXOQUILL_PIPER_VOICE = Join-Path $runtimes "piper-voices\de_DE-thorsten-high.onnx"
# Auto-start the experimental XTTS-v2 sidecar → XTTS becomes the default voice
# (Piper stays the fallback until it warms up / if it fails). Comment these two
# out to use Piper only. Setup once with scripts/setup-xtts.ps1.
$env:EXOQUILL_XTTS_PYTHON = Join-Path $root ".venv-xtts\Scripts\python.exe"
$env:EXOQUILL_XTTS_SCRIPT = Join-Path $root "scripts\xtts-server.py"
# Auto-start the experimental Zonos-v0.1 sidecar (Apache-2.0 weights, CUDA GPU).
# It only activates once scripts/setup-zonos.ps1 has run and the voices folder
# holds reference .wav clips; until then these paths don't exist and it's skipped.
$env:EXOQUILL_ZONOS_PYTHON = Join-Path $root ".venv-zonos\Scripts\python.exe"
$env:EXOQUILL_ZONOS_SCRIPT = Join-Path $root "scripts\zonos-server.py"
$env:EXOQUILL_ZONOS_VOICES = Join-Path $root "zonos-voices"
$env:EXOQUILL_WHISPER = Join-Path $runtimes "whisper\whisper-cli.exe"
$env:EXOQUILL_WHISPER_MODEL = Join-Path $runtimes "models\ggml-large-v3-turbo-q5_0.bin"
# Optional Silero neural VAD (only used in a `--features silero` build); harmless
# to set otherwise. Fetch the assets with scripts/fetch-silero.ps1.
$env:EXOQUILL_SILERO_MODEL = Join-Path $runtimes "models\silero_vad.onnx"
$env:ORT_DYLIB_PATH = Join-Path $runtimes "onnxruntime\onnxruntime.dll"
# Native Kokoro TTS (built by default now). German (Martin) is the OOTB voice;
# English is optional. Missing assets are simply skipped at runtime. Fetch with
# scripts/fetch-kokoro.ps1 (also pulls the CPU onnxruntime + portable espeak-ng).
$env:EXOQUILL_KOKORO_DE_MODEL = Join-Path $runtimes "models\kokoro-de\kokoro-martin.onnx"
$env:EXOQUILL_KOKORO_DE_VOICES = Join-Path $runtimes "models\kokoro-de\voices-martin.npz"
$env:EXOQUILL_KOKORO_MODEL = Join-Path $runtimes "models\kokoro\model.onnx"
$env:EXOQUILL_KOKORO_VOICES = Join-Path $runtimes "models\kokoro\voices"
$env:EXOQUILL_ESPEAK = Join-Path $runtimes "espeak-ng\espeak-ng.exe"
$env:EXOQUILL_ESPEAK_DATA = Join-Path $runtimes "espeak-ng"

Set-Location $root
pnpm dev
