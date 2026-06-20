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
$env:EXOQUILL_PIPER = Join-Path $runtimes "piper\piper.exe"
$env:EXOQUILL_PIPER_VOICE = Join-Path $runtimes "piper-voices\de_DE-thorsten-medium.onnx"
$env:EXOQUILL_WHISPER = Join-Path $runtimes "whisper\whisper-cli.exe"
$env:EXOQUILL_WHISPER_MODEL = Join-Path $runtimes "models\ggml-large-v3-turbo-q5_0.bin"
# Optional Silero neural VAD (only used in a `--features silero` build); harmless
# to set otherwise. Fetch the assets with scripts/fetch-silero.ps1.
$env:EXOQUILL_SILERO_MODEL = Join-Path $runtimes "models\silero_vad.onnx"
$env:ORT_DYLIB_PATH = Join-Path $runtimes "onnxruntime\onnxruntime.dll"

Set-Location $root
pnpm dev
