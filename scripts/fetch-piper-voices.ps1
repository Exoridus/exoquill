# Fetches a curated set of high-quality (`high` tier) Piper TTS voices into
# runtimes/piper-voices/, where dev.ps1 points ExoQuill. Each voice is an
# `.onnx` model plus its `.onnx.json` config (the config carries the sample rate
# the provider reads per voice). Like the other AI assets these are bundled as
# Tauri resources for release and are not in git.
#
#   pwsh scripts/fetch-piper-voices.ps1            # missing voices only
#   pwsh scripts/fetch-piper-voices.ps1 -Force     # re-download everything
#   pwsh scripts/fetch-piper-voices.ps1 -Prune     # also delete non-curated voices
#
# Only `high` models are used here (audiobook-grade). German has just one high
# voice (Thorsten); the others are English. The provider discovers whatever
# lands in the folder; -Prune removes anything not listed below. Voices come from
# the rhasspy/piper-voices repo on Hugging Face.
#
# Licensing (verify before bundling — decisions D2 open item): the Thorsten
# dataset is CC0; the English voices derive from other corpora with their own
# terms. Each voice's MODEL_CARD on Hugging Face states its license.

param(
    [switch]$Force,
    [switch]$Prune
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$voicesDir = Join-Path $root "runtimes\piper-voices"
New-Item -ItemType Directory -Force -Path $voicesDir | Out-Null

$base = "https://huggingface.co/rhasspy/piper-voices/resolve/main"

# lang = HF top-level family dir; code-name-quality form the file stem + sub-path.
$voices = @(
    @{ Lang = "de"; Code = "de_DE"; Name = "thorsten"; Quality = "high" },
    @{ Lang = "en"; Code = "en_US"; Name = "lessac";   Quality = "high" },
    @{ Lang = "en"; Code = "en_US"; Name = "ryan";     Quality = "high" },
    @{ Lang = "en"; Code = "en_GB"; Name = "cori";     Quality = "high" }
)

$keep = $voices | ForEach-Object { "$($_.Code)-$($_.Name)-$($_.Quality)" }

if ($Prune) {
    Get-ChildItem -Path $voicesDir -Filter "*.onnx" | ForEach-Object {
        if ($keep -notcontains $_.BaseName) {
            Write-Host "Pruning $($_.Name) ..."
            Remove-Item $_.FullName -Force
            $json = "$($_.FullName).json"
            if (Test-Path $json) { Remove-Item $json -Force }
        }
    }
}

foreach ($v in $voices) {
    $stem = "$($v.Code)-$($v.Name)-$($v.Quality)"
    $dir = "$($v.Lang)/$($v.Code)/$($v.Name)/$($v.Quality)"
    foreach ($ext in @("onnx", "onnx.json")) {
        $dest = Join-Path $voicesDir "$stem.$ext"
        if ($Force -or -not (Test-Path $dest)) {
            $url = "$base/$dir/$stem.$ext"
            Write-Host "Downloading $stem.$ext ..."
            Invoke-WebRequest -Uri $url -OutFile $dest
        } else {
            Write-Host "Already present: $stem.$ext"
        }
    }
}

Write-Host "Done. $($voices.Count) high-quality voices in $voicesDir. Restart the app to pick them up."
