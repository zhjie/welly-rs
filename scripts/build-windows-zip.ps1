param(
    [string]$Target = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$AppName = "Welly-rs"
$CrateName = "welly-rs"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).Path
$TargetDir = Join-Path $RepoRoot "target\release"
$BundleDir = Join-Path $TargetDir "bundle\windows"
$PackageDir = Join-Path $BundleDir $AppName
$ExeName = "$CrateName.exe"
$ExePath = Join-Path $TargetDir $ExeName

Set-Location $RepoRoot

if ($Target -ne "") {
    $TargetDir = Join-Path $RepoRoot "target\$Target\release"
    $BundleDir = Join-Path $TargetDir "bundle\windows"
    $PackageDir = Join-Path $BundleDir $AppName
    $ExePath = Join-Path $TargetDir $ExeName
}

if (-not $SkipBuild) {
    $BuildArgs = @("build", "--release")
    if ($Target -ne "") {
        $BuildArgs += @("--target", $Target)
    }
    & cargo @BuildArgs
}

if (-not (Test-Path $ExePath)) {
    throw "Executable not found: $ExePath"
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
