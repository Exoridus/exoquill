# Fetches the whisper.cpp Windows runtime + a ggml model into runtimes/, where
# dev.ps1 points ExoQuill's dictation feature (decisions D5/D8). Like the other
# AI runtimes these are bundled as Tauri resources for release and are not in git.
#
#   pwsh scripts/fetch-whisper.ps1            # base model (good German default)
#   pwsh scripts/fetch-whisper.ps1 -Model small
#
# Re-running is idempotent: existing files are kept unless -Force is given.

param(
    [ValidateSet("tiny", "base", "small", "medium")]
    [string]$Model = "base",
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$runtimes = Join-Path $root "runtimes"
$whisperDir = Join-Path $runtimes "whisper"
$modelsDir = Join-Path $runtimes "models"
New-Item -ItemType Directory -Force -Path $whisperDir, $modelsDir | Out-Null

# --- Model -------------------------------------------------------------------
$modelName = "ggml-$Model.bin"
$modelPath = Join-Path $modelsDir $modelName
if ($Force -or -not (Test-Path $modelPath)) {
    $modelUrl = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$modelName"
    Write-Host "Downloading $modelName ..."
    Invoke-WebRequest -Uri $modelUrl -OutFile $modelPath
} else {
    Write-Host "Model already present: $modelPath"
}

# --- Runtime (whisper-cli.exe + ggml DLLs) -----------------------------------
$cli = Join-Path $whisperDir "whisper-cli.exe"
if ($Force -or -not (Test-Path $cli)) {
    Write-Host "Resolving latest whisper.cpp Windows release ..."
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/ggml-org/whisper.cpp/releases/latest" `
        -Headers @{ "User-Agent" = "exoquill-fetch" }
    # Prefer the plain CPU x64 build; it needs no CUDA/BLAS runtime.
    $asset = $release.assets |
        Where-Object { $_.name -match "bin-x64\.zip$" -and $_.name -notmatch "cublas|blas|clblast" } |
        Select-Object -First 1
    if (-not $asset) {
        $asset = $release.assets | Where-Object { $_.name -match "bin-x64\.zip$" } | Select-Object -First 1
    }
    if (-not $asset) { throw "No Windows x64 asset found in the latest whisper.cpp release." }

    $zipPath = Join-Path $env:TEMP $asset.name
    Write-Host "Downloading $($asset.name) ..."
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath
    Expand-Archive -Path $zipPath -DestinationPath $whisperDir -Force
    Remove-Item $zipPath -Force

    # whisper.cpp Windows zips ship the real binaries under Release\ alongside
    # tiny deprecation-stub .exes at the top level (running a stub just prints a
    # warning and exits non-zero). Pick the real whisper-cli.exe (the stub is
    # only a few KB) and flatten its folder up so the resolver finds
    # whisper\whisper-cli.exe next to its ggml/whisper DLLs.
    $real = Get-ChildItem -Path $whisperDir -Recurse -Filter "whisper-cli.exe" |
        Sort-Object Length -Descending | Select-Object -First 1
    if (-not $real) { throw "whisper-cli.exe not found in the extracted archive." }
    if ($real.DirectoryName -ne $whisperDir) {
        Get-ChildItem -Path $real.DirectoryName -File | Copy-Item -Destination $whisperDir -Force
        Remove-Item -Path (Join-Path $whisperDir "Release") -Recurse -Force -ErrorAction SilentlyContinue
    }
} else {
    Write-Host "Runtime already present: $cli"
}

Write-Host "Done. Dictation will use $modelPath via $cli."
