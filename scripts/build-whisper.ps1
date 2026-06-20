# Builds whisper.cpp from source with GPU backends so ExoQuill's dictation runs
# on the GPU. whisper-cli's main() calls ggml_backend_load_all(), which loads
# every ggml-*.dll next to the executable at runtime and auto-selects the fastest
# available device (CUDA > Vulkan > CPU). One runtime dir therefore covers NVIDIA,
# AMD/Intel and CPU-only machines (decisions D5/D8); a backend whose driver or
# runtime is missing is skipped gracefully instead of crashing the app.
#
# Backends are auto-detected from the toolchains present:
#   - CUDA   when CUDA_PATH is set and nvcc is found   (NVIDIA, fastest)
#   - Vulkan when VULKAN_SDK is set and glslc is found (cross-vendor: AMD/Intel/NVIDIA)
# At least one GPU backend must be available; the CPU backend is always built as
# a fallback. The output goes to runtimes/whisper/, where dev.ps1 points
# EXOQUILL_WHISPER (and where release bundles it as a Tauri resource).
#
#   pwsh scripts/build-whisper.ps1                  # auto-detect backends, build
#   pwsh scripts/build-whisper.ps1 -Tag v1.9.1      # pin a different whisper.cpp tag
#   pwsh scripts/build-whisper.ps1 -CudaArch 120    # explicit arch (e.g. Blackwell)
#   pwsh scripts/build-whisper.ps1 -Force           # re-clone + clean rebuild
#
# Requires: git, cmake, and the MSVC C++ toolchain (Visual Studio / Build Tools).

param(
    [string]$Tag = "v1.9.1",
    # CUDA target architecture. "native" auto-detects the GPU in this machine
    # (fast, dev default). For a redistributable build pass an explicit list,
    # e.g. "75;80;86;89;120".
    [string]$CudaArch = "native",
    [switch]$NoCuda,
    [switch]$NoVulkan,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$whisperDir = Join-Path $root "runtimes\whisper"
# Lives under build/ so .gitignore's `build/` rule keeps the clone + artifacts
# out of git.
$workDir = Join-Path $root ".workspace\build\whisper.cpp"
$buildDir = Join-Path $workDir "build"

function Require-Tool($name, $hint) {
    if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
        throw "$name not found on PATH. $hint"
    }
}
Require-Tool git   "Install Git."
Require-Tool cmake "Install CMake (https://cmake.org)."

# --- Backend detection -------------------------------------------------------
$useCuda = (-not $NoCuda) -and [bool]$env:CUDA_PATH -and [bool](Get-Command nvcc -ErrorAction SilentlyContinue)
$useVulkan = (-not $NoVulkan) -and [bool]$env:VULKAN_SDK -and [bool](Get-Command glslc -ErrorAction SilentlyContinue)
if (-not ($useCuda -or $useVulkan)) {
    throw @"
No GPU backend available.
  CUDA   needs CUDA_PATH + nvcc   (CUDA_PATH='$($env:CUDA_PATH)')
  Vulkan needs VULKAN_SDK + glslc (VULKAN_SDK='$($env:VULKAN_SDK)')
Install the Vulkan SDK (https://vulkan.lunarg.com) and/or the CUDA Toolkit, then re-run.
"@
}
Write-Host "Backends -> CUDA=$useCuda  Vulkan=$useVulkan  (CPU always built)"

# --- MSVC toolchain ----------------------------------------------------------
# Build with the Ninja generator, not the Visual Studio generator: CUDA's VS
# MSBuild integration ("CUDA <ver>.props") isn't installed for brand-new VS
# versions, which makes the VS generator fail with "No CUDA toolset found".
# Ninja invokes nvcc directly, so it only needs cl.exe + nvcc on PATH — which we
# get by entering the VS developer environment.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw "vswhere not found. Install Visual Studio (with the C++ toolchain)." }
$vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsPath) { throw "No Visual Studio with the MSVC C++ toolchain found." }
$devShell = Join-Path $vsPath "Common7\Tools\Microsoft.VisualStudio.DevShell.dll"
Import-Module $devShell
Enter-VsDevShell -VsInstallPath $vsPath -SkipAutomaticLocation -DevCmdArguments "-arch=x64 -host_arch=x64" | Out-Null
# nvcc needs the CUDA toolkit bin on PATH (Enter-VsDevShell keeps the existing PATH).
if ($useCuda) {
    $env:PATH = "$(Join-Path $env:CUDA_PATH 'bin');$env:PATH"
    # CUDA's nvcc gates the host compiler to MSVC versions it was released with
    # (CUDA 13 -> VS 2019-2022). A newer Visual Studio (e.g. 2026) trips the
    # `unsupported Microsoft Visual Studio version` #error. NVCC_APPEND_FLAGS
    # applies to every nvcc call, including CMake's compiler-id probe, so the
    # override lands before the version gate. Drop this once the CUDA Toolkit
    # supports the installed MSVC.
    $env:NVCC_APPEND_FLAGS = "-allow-unsupported-compiler"
}

$ninja = (Get-Command ninja -ErrorAction SilentlyContinue)?.Source
if (-not $ninja) { $ninja = Join-Path $vsPath "Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe" }
if (-not (Test-Path $ninja)) { throw "Ninja not found (install it or the VS 'C++ CMake tools' component)." }

# --- Source ------------------------------------------------------------------
New-Item -ItemType Directory -Force -Path (Split-Path $workDir -Parent) | Out-Null
if ($Force -and (Test-Path $workDir)) { Remove-Item -Recurse -Force $workDir }
if (-not (Test-Path $workDir)) {
    Write-Host "Cloning whisper.cpp $Tag ..."
    git clone --depth 1 --branch $Tag https://github.com/ggml-org/whisper.cpp $workDir
} else {
    Write-Host "Reusing source at $workDir (pass -Force to re-clone)."
}

# --- Configure ---------------------------------------------------------------
if ($Force -and (Test-Path $buildDir)) { Remove-Item -Recurse -Force $buildDir }
$cmakeArgs = @(
    "-B", $buildDir, "-S", $workDir,
    "-G", "Ninja",
    "-DCMAKE_MAKE_PROGRAM=$ninja",
    "-DCMAKE_BUILD_TYPE=Release",
    "-DBUILD_SHARED_LIBS=ON",     # required by GGML_BACKEND_DL
    "-DGGML_BACKEND_DL=ON",       # backends as standalone, runtime-loaded DLLs
    # GGML_BACKEND_DL rejects GGML_NATIVE; the CPU backend must be built as
    # runtime-selectable variants, which also makes the CPU fallback portable
    # across x86-64 machines.
    "-DGGML_CPU_ALL_VARIANTS=ON",
    "-DGGML_NATIVE=OFF",
    "-DWHISPER_BUILD_EXAMPLES=ON",# builds whisper-cli
    "-DWHISPER_BUILD_TESTS=OFF",
    "-DWHISPER_BUILD_SERVER=OFF"
)
if ($useVulkan) { $cmakeArgs += "-DGGML_VULKAN=ON" }
if ($useCuda) {
    $cmakeArgs += "-DGGML_CUDA=ON", "-DCMAKE_CUDA_ARCHITECTURES=$CudaArch",
        "-DCMAKE_CUDA_FLAGS=-allow-unsupported-compiler"
}

Write-Host "Configuring ..."
cmake @cmakeArgs

# --- Build -------------------------------------------------------------------
Write-Host "Building (Release) — this can take several minutes for CUDA ..."
cmake --build $buildDir -j

# --- Collect artifacts -------------------------------------------------------
# Start from a clean dir so a previous build's (or an old prebuilt's) stale DLLs
# can't shadow this build — ggml_backend_load_all loads every ggml-*.dll present.
if (Test-Path $whisperDir) { Remove-Item -Recurse -Force $whisperDir }
New-Item -ItemType Directory -Force -Path $whisperDir | Out-Null
# Multi-config (Visual Studio) writes to build\bin\Release; single-config to build\bin.
$binDir = Join-Path $buildDir "bin\Release"
if (-not (Test-Path $binDir)) { $binDir = Join-Path $buildDir "bin" }
$cli = Join-Path $binDir "whisper-cli.exe"
if (-not (Test-Path $cli)) { throw "whisper-cli.exe not found under $binDir after build." }

# whisper-cli.exe (per-call) + whisper-server.exe (persistent, for live
# streaming dictation) + whisper.dll + every ggml*.dll (core + each backend).
Copy-Item $cli -Destination $whisperDir -Force
$server = Join-Path $binDir "whisper-server.exe"
if (Test-Path $server) { Copy-Item $server -Destination $whisperDir -Force }
Get-ChildItem -Path $binDir -Filter "*.dll" | Copy-Item -Destination $whisperDir -Force

# ggml-cuda.dll links against the CUDA runtime; bundle it so the runtime works on
# machines without the CUDA Toolkit installed. (Vulkan needs only vulkan-1.dll,
# which ships with the GPU driver.)
if ($useCuda) {
    # CUDA 12 ships the runtime DLLs under bin\; CUDA 13 moved them to bin\x64\.
    $cudaSearch = @((Join-Path $env:CUDA_PATH "bin"), (Join-Path $env:CUDA_PATH "bin\x64")) |
        Where-Object { Test-Path $_ }
    $copied = 0
    foreach ($pattern in @("cudart64_*.dll", "cublas64_*.dll", "cublasLt64_*.dll")) {
        foreach ($dir in $cudaSearch) {
            Get-ChildItem -Path $dir -Filter $pattern -ErrorAction SilentlyContinue | ForEach-Object {
                Copy-Item $_.FullName -Destination $whisperDir -Force
                $copied++
            }
        }
    }
    if ($copied -eq 0) {
        Write-Warning "No CUDA runtime DLLs (cudart/cublas) found under $env:CUDA_PATH; the bundled runtime will need them on PATH to use CUDA."
    }
}

Write-Host ""
Write-Host "Done. Runtime in $whisperDir"
Get-ChildItem -Path $whisperDir -Filter "ggml-*.dll" |
    ForEach-Object { Write-Host "  backend dll: $($_.Name)" }
Write-Host "Dictation picks the fastest available device at runtime (CUDA > Vulkan > CPU)."
