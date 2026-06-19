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

    # Release zips sometimes nest binaries in a subfolder and/or still ship the
    # CLI as main.exe; normalize so the resolver finds whisper\whisper-cli.exe.
    if (-not (Test-Path $cli)) {
        $found = Get-ChildItem -Path $whisperDir -Recurse -File |
            Where-Object { $_.Name -in @("whisper-cli.exe", "main.exe") } |
            Select-Object -First 1
        if (-not $found) { throw "whisper-cli.exe / main.exe not found in the extracted archive." }
        Get-ChildItem -Path $found.DirectoryName -File | Copy-Item -Destination $whisperDir -Force
        Copy-Item -Path $found.FullName -Destination $cli -Force
    }
} else {
    Write-Host "Runtime already present: $cli"
}

Write-Host "Done. Dictation will use $modelPath via $cli."
