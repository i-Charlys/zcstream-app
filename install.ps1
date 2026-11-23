#Requires -Version 5.1
<#
.SYNOPSIS
    Zero-config one-command installer & builder for the complete zcstream / RUDP video streaming suite.
.DESCRIPTION
    Automatically clones or downloads the 5 sibling repositories (zcrudp, zcvid, zpsync, zcstream, zcstream-app),
    detects your C/C++ toolchain (Clang/MinGW, GCC/MinGW, Clang/Ninja, or MSVC), downloads optional headers
    (nvEncodeAPI.h), builds all libraries, CLI binaries, and the zcstream-app GUI, runs the test suites, and
    stages a ready-to-run portable bundle in ./dist/.

    Works whether invoked from a git clone, a GitHub "Download ZIP" folder (<repo>-main), or an empty folder,
    and falls back to GitHub ZIP archives automatically if Git is not installed.

    Usage:
      .\install.ps1
      .\install.ps1 -SkipTests
      .\install.ps1 -WorkspaceDir C:\path\to\RUDP
#>
[CmdletBinding()]
param(
    [string]$WorkspaceDir = "",
    [switch]$SkipTests,
    [switch]$PortableCodec,
    [switch]$NoPull
)

$ErrorActionPreference = 'Stop'

$ScriptDir = if ($PSScriptRoot) { $PSScriptRoot } else { (Get-Location).Path }

# Determine workspace root: if invoked from inside one of the 5 repos (including GitHub ZIP <repo>-main folders
# or custom clone folder names), use its parent folder so all 5 sibling repos live side-by-side cleanly.
if ([string]::IsNullOrWhiteSpace($WorkspaceDir)) {
    $Leaf = Split-Path -Leaf $ScriptDir
    $IsSingleRepoDir = (
        ($Leaf -match '^(zcstream-app|zcstream|zcrudp|zcvid|zpsync|scripts)(-[A-Za-z0-9._-]+)?$') -or
        ((Test-Path (Join-Path $ScriptDir 'Cargo.toml')) -and -not (Test-Path (Join-Path $ScriptDir 'zcstream'))) -or
        ((Test-Path (Join-Path $ScriptDir 'src\stream_live.c')) -and -not (Test-Path (Join-Path $ScriptDir 'zcstream')))
    )
    if ($IsSingleRepoDir) {
        $WorkspaceDir = Split-Path -Parent $ScriptDir
    } else {
        $WorkspaceDir = $ScriptDir
    }
}

New-Item -ItemType Directory -Force -Path $WorkspaceDir | Out-Null
$Root = (Resolve-Path $WorkspaceDir).Path
Write-Host "=== zcstream / RUDP Unified Installer ===" -ForegroundColor Cyan
Write-Host "Workspace : $Root"

function Test-Tool([string]$Name) {
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

$HasGit = Test-Tool 'git'
if (-not (Test-Tool 'cmake')) {
    throw "CMake (>= 3.21) is required but was not found in PATH. Install CMake from https://cmake.org/"
}

$Repos = @(
    @{ Name = 'zcrudp';       Url = 'https://github.com/i-Charlys/zcrudp.git';       Check = 'include\rudp_nextgen.h' },
    @{ Name = 'zcvid';        Url = 'https://github.com/i-Charlys/zcvid.git';        Check = 'include\zcvid_codec.h' },
    @{ Name = 'zpsync';       Url = 'https://github.com/i-Charlys/zpsync.git';       Check = 'include\zpsync.h' },
    @{ Name = 'zcstream';     Url = 'https://github.com/i-Charlys/zcstream.git';     Check = 'src\stream_live.c' },
    @{ Name = 'zcstream-app'; Url = 'https://github.com/i-Charlys/zcstream-app.git'; Check = 'Cargo.toml' }
)

function Install-RepoFromZip([string]$Name, [string]$GitUrl, [string]$DestPath, [string]$CheckRel) {
    $BaseUrl = $GitUrl -replace '\.git$', ''
    $ZipUrl = "$BaseUrl/archive/refs/heads/main.zip"
    $TmpZip = Join-Path $env:TEMP "$Name-$([Guid]::NewGuid().ToString('N')).zip"
    $TmpExtract = Join-Path $env:TEMP "$Name-extract-$([Guid]::NewGuid().ToString('N'))"
    try {
        Write-Host "[zip] Downloading $Name from $ZipUrl..." -ForegroundColor Yellow
        Invoke-WebRequest -Uri $ZipUrl -OutFile $TmpZip -UseBasicParsing
        Expand-Archive -Path $TmpZip -DestinationPath $TmpExtract -Force
        $SubDir = Get-ChildItem -Path $TmpExtract -Directory | Select-Object -First 1
        if (-not $SubDir -or -not (Test-Path (Join-Path $SubDir.FullName $CheckRel))) {
            throw "Downloaded archive for $Name did not contain expected $CheckRel"
        }
        if (Test-Path $DestPath) {
            Remove-Item -Recurse -Force $DestPath
        }
        Move-Item -Path $SubDir.FullName -Destination $DestPath
    } finally {
        Remove-Item -Force $TmpZip -ErrorAction SilentlyContinue
        Remove-Item -Recurse -Force $TmpExtract -ErrorAction SilentlyContinue
    }
}

$RepoDirs = @{}

foreach ($r in $Repos) {
    $Candidates = @(
        (Join-Path $Root $r.Name),
        (Join-Path $Root "$($r.Name)-main"),
        (Join-Path $Root "$($r.Name)-master"),
        $ScriptDir
    )
    $FoundPath = $null
    foreach ($c in $Candidates) {
        if ($c -and (Test-Path (Join-Path $c $r.Check))) {
            $FoundPath = (Resolve-Path $c).Path
            break
        }
    }

    if ($FoundPath) {
        $RepoPath = $FoundPath
        if ($HasGit -and (Test-Path (Join-Path $RepoPath '.git'))) {
            if (-not $NoPull) {
                $Dirty = git -C $RepoPath status --porcelain
                if ([string]::IsNullOrWhiteSpace($Dirty)) {
                    Write-Host "[git] Updating $($r.Name)..." -ForegroundColor DarkGray
                    git -C $RepoPath pull --ff-only 2>$null | Out-Null
                } else {
                    Write-Host "[git] $($r.Name) has local changes, keeping working tree as-is." -ForegroundColor DarkGray
                }
            }
        } else {
            Write-Host "[repo] Using existing $($r.Name) at $RepoPath" -ForegroundColor DarkGray
        }
    } else {
        $RepoPath = Join-Path $Root $r.Name
        $Cloned = $false
        if ($HasGit) {
            Write-Host "[git] Cloning $($r.Name) from $($r.Url)..." -ForegroundColor Yellow
            if (Test-Path $RepoPath) {
                Remove-Item -Recurse -Force $RepoPath
            }
            git clone $r.Url $RepoPath
            if ($LASTEXITCODE -eq 0 -and (Test-Path (Join-Path $RepoPath $r.Check))) {
                $Cloned = $true
            }
        }
        if (-not $Cloned) {
            Install-RepoFromZip $r.Name $r.Url $RepoPath $r.Check
        }
    }
    $RepoDirs[$r.Name] = $RepoPath
}

# Detect best C/C++ generator & compiler on Windows
$CMakeGenArgs = @()
if ((Test-Tool 'clang') -and (Test-Tool 'clang++') -and (Test-Tool 'mingw32-make')) {
    Write-Host "[toolchain] Using Clang + MinGW Makefiles" -ForegroundColor Green
    $CMakeGenArgs = @('-G', 'MinGW Makefiles', '-DCMAKE_C_COMPILER=clang', '-DCMAKE_CXX_COMPILER=clang++', '-DCMAKE_BUILD_TYPE=Release')
} elseif ((Test-Tool 'gcc') -and (Test-Tool 'g++') -and (Test-Tool 'mingw32-make')) {
    Write-Host "[toolchain] Using GCC + MinGW Makefiles" -ForegroundColor Green
    $CMakeGenArgs = @('-G', 'MinGW Makefiles', '-DCMAKE_C_COMPILER=gcc', '-DCMAKE_CXX_COMPILER=g++', '-DCMAKE_BUILD_TYPE=Release')
} elseif ((Test-Tool 'clang') -and (Test-Tool 'clang++') -and (Test-Tool 'ninja')) {
    Write-Host "[toolchain] Using Clang + Ninja" -ForegroundColor Green
    $CMakeGenArgs = @('-G', 'Ninja', '-DCMAKE_C_COMPILER=clang', '-DCMAKE_CXX_COMPILER=clang++', '-DCMAKE_BUILD_TYPE=Release')
} else {
    Write-Host "[toolchain] Using default CMake generator (MSVC / Visual Studio)" -ForegroundColor Green
}

$Jobs = [Environment]::ProcessorCount
if ($Jobs -lt 1) { $Jobs = 4 }

function Invoke-CMakeBuild([string]$Name, [string]$SourceDir, [string[]]$ExtraArgs) {
    $BuildDir = Join-Path $SourceDir 'build'
    Write-Host "`n=== Building $Name ===" -ForegroundColor Cyan
    $CfgArgs = @('-S', $SourceDir, '-B', $BuildDir) + $CMakeGenArgs + $ExtraArgs
    & cmake @CfgArgs
    if ($LASTEXITCODE -ne 0) { throw "CMake configure failed for $Name" }

    & cmake --build $BuildDir --config Release -j $Jobs
    if ($LASTEXITCODE -ne 0) { throw "CMake build failed for $Name" }

    if (-not $SkipTests) {
        $CtestArgs = @('--test-dir', $BuildDir, '--output-on-failure')
        if (Test-Path (Join-Path $BuildDir 'Release')) {
            $CtestArgs += @('-C', 'Release')
        }
        & ctest @CtestArgs
        if ($LASTEXITCODE -ne 0) { throw "Tests failed for $Name" }
    }
}

$ZcrudpDir   = $RepoDirs['zcrudp']
$ZcvidDir    = $RepoDirs['zcvid']
$ZpsyncDir   = $RepoDirs['zpsync']
$ZcstreamDir = $RepoDirs['zcstream']
$AppDir      = $RepoDirs['zcstream-app']

Invoke-CMakeBuild 'zcrudp' $ZcrudpDir @()
Invoke-CMakeBuild 'zcvid'  $ZcvidDir  @()
Invoke-CMakeBuild 'zpsync' $ZpsyncDir @("-DZCRUDP_DIR=$ZcrudpDir")

$ZcstreamExtra = @(
    "-DZCRUDP_DIR=$ZcrudpDir",
    "-DZCVID_DIR=$ZcvidDir",
    "-DZPSYNC_DIR=$ZpsyncDir"
)
if ($PortableCodec) {
    $ZcstreamExtra += '-DZCRUDP_PORTABLE_CODEC=ON'
}
Invoke-CMakeBuild 'zcstream' $ZcstreamDir $ZcstreamExtra

$DistDir = Join-Path $Root 'dist'
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
& cmake --install (Join-Path $ZcstreamDir 'build') --config Release --prefix $DistDir
if ($LASTEXITCODE -ne 0) { throw "CMake install failed for zcstream" }

if (Test-Tool 'cargo') {
    Write-Host "`n=== Building zcstream-app (GUI Control Center) ===" -ForegroundColor Cyan
    Push-Location $AppDir
    try {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed for zcstream-app" }
        if (-not $SkipTests) {
            & cargo test --release
            if ($LASTEXITCODE -ne 0) { throw "cargo test failed for zcstream-app" }
        }
        $AppExe = Join-Path $AppDir 'target\release\zcstream-app.exe'
        if (Test-Path $AppExe) {
            Copy-Item -Force $AppExe (Join-Path $DistDir 'zcstream-app.exe')
        }
    } finally {
        Pop-Location
    }
} else {
    Write-Host "[warn] Cargo (Rust) not found in PATH — skipping zcstream-app GUI build. Install Rust from https://rustup.rs/" -ForegroundColor Yellow
}

Write-Host "`n=== Installation Complete ===" -ForegroundColor Green
Write-Host "All binaries and shaders are staged in: $DistDir"
Get-ChildItem -Path $DistDir -File | ForEach-Object { Write-Host "  - $($_.Name)" }
Write-Host "`nRun the Control Center GUI:" -ForegroundColor Cyan
Write-Host "  `"$DistDir\zcstream-app.exe`""
