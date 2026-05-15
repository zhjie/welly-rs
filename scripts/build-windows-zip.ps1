param(
    [string]$Target = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$AppName = "Welly-rs"
$CrateName = "welly-rs"
$DefaultMsvcTarget = "x86_64-pc-windows-msvc"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).Path
$TargetDir = Join-Path $RepoRoot "target\release"
$BundleDir = Join-Path $TargetDir "bundle\windows"
$PackageDir = Join-Path $BundleDir $AppName
$ExeName = "$CrateName.exe"
$ExePath = Join-Path $TargetDir $ExeName
$HostReleaseDir = Join-Path $RepoRoot "target\release"
$PdbName = "{0}.pdb" -f $CrateName.Replace("-", "_")
$PdbPath = Join-Path $TargetDir $PdbName
$EffectiveTarget = if ($Target -ne "") { $Target } else { $DefaultMsvcTarget }

Set-Location $RepoRoot

function Get-BuildInputPaths {
    @(
        (Join-Path $RepoRoot "Cargo.toml")
        (Join-Path $RepoRoot "Cargo.lock")
        (Join-Path $RepoRoot "build.rs")
        (Join-Path $RepoRoot "assets\welly-rs-icon.svg")
        (Join-Path $RepoRoot "scripts\make-app-icons.py")
    ) + (Get-ChildItem (Join-Path $RepoRoot "src") -Recurse -File | Select-Object -ExpandProperty FullName)
}

function Assert-BuildArtifactIsFresh {
    param(
        [string]$ArtifactPath
    )

    if (-not (Test-Path $ArtifactPath)) {
        return
    }

    $artifactWriteTime = (Get-Item $ArtifactPath).LastWriteTimeUtc
    $newerInput = Get-BuildInputPaths |
        Where-Object { Test-Path $_ } |
        Get-Item |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1

    if ($null -ne $newerInput -and $newerInput.LastWriteTimeUtc -gt $artifactWriteTime) {
        throw "SkipBuild would package a stale executable. '$($newerInput.FullName)' is newer than '$ArtifactPath'. Re-run without -SkipBuild."
    }
}

if ($Target -ne "") {
    $TargetDir = Join-Path $RepoRoot "target\$Target\release"
    $BundleDir = Join-Path $TargetDir "bundle\windows"
    $PackageDir = Join-Path $BundleDir $AppName
    $ExePath = Join-Path $TargetDir $ExeName
    $PdbPath = Join-Path $TargetDir $PdbName
}

if ($SkipBuild) {
    Assert-BuildArtifactIsFresh -ArtifactPath $ExePath
}

if (-not $SkipBuild) {
    $BuildArgs = @("build", "--release")
    if ($Target -ne "") {
        $BuildArgs += @("--target", $Target)
    }

    $RustFlagsVar = "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"
    $PreviousRustFlags = [Environment]::GetEnvironmentVariable($RustFlagsVar, "Process")
    $RestoreRustFlags = $null -ne $PreviousRustFlags

    try {
        if ($EffectiveTarget -eq $DefaultMsvcTarget) {
            [Environment]::SetEnvironmentVariable(
                $RustFlagsVar,
                "-C target-feature=+crt-static",
                "Process"
            )
        }

        & cargo @BuildArgs
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    }
    finally {
        if ($RestoreRustFlags) {
            [Environment]::SetEnvironmentVariable($RustFlagsVar, $PreviousRustFlags, "Process")
        }
        else {
            [Environment]::SetEnvironmentVariable($RustFlagsVar, $null, "Process")
        }
    }
}

if (-not (Test-Path $ExePath)) {
    throw "Executable not found: $ExePath"
}

if ($TargetDir -ne $HostReleaseDir) {
    New-Item -ItemType Directory -Force $HostReleaseDir | Out-Null
    Copy-Item $ExePath (Join-Path $HostReleaseDir $ExeName)
    if (Test-Path $PdbPath) {
        Copy-Item $PdbPath (Join-Path $HostReleaseDir $PdbName)
    }
}

if (Test-Path $PackageDir) {
    Remove-Item -Recurse -Force $PackageDir
}
New-Item -ItemType Directory -Force $PackageDir | Out-Null

Copy-Item $ExePath (Join-Path $PackageDir $ExeName)
Copy-Item (Join-Path $RepoRoot "README.md") (Join-Path $PackageDir "README.md")
Copy-Item (Join-Path $RepoRoot "LICENSE") (Join-Path $PackageDir "LICENSE")

$ZipSuffix = if ($Target -ne "") { $Target } else { "windows" }
$ZipPath = Join-Path $BundleDir "$AppName-$ZipSuffix.zip"
if (Test-Path $ZipPath) {
    Remove-Item -Force $ZipPath
}

Compress-Archive -Path (Join-Path $PackageDir "*") -DestinationPath $ZipPath

Write-Host "Built $ZipPath"
