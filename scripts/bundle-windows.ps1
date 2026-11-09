# Build qBit for Windows and package it as a zip.
#
# Usage: scripts/bundle-windows.ps1 [version]
#
# Builds for the machine it runs on (x64 or arm64); set QBIT_TARGET to
# x86_64-pc-windows-msvc or aarch64-pc-windows-msvc to override. Writes
# dist/qBit-<version>-windows-<arch>.zip, holding `qbit.exe`. The version defaults to
# the one in Cargo.toml.
#
# Needs the MSVC build tools (Visual Studio's "Desktop development with C++" workload).
param([string]$Version)

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

if (-not $Version) {
    $Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' |
        Select-Object -First 1).Matches[0].Groups[1].Value
}
$Version = $Version -replace '^v', ''

$target = $env:QBIT_TARGET
if (-not $target) {
    switch ($env:PROCESSOR_ARCHITECTURE) {
        "AMD64" { $target = "x86_64-pc-windows-msvc" }
        "ARM64" { $target = "aarch64-pc-windows-msvc" }
        default { throw "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE; set QBIT_TARGET" }
    }
}

switch -Wildcard ($target) {
    "x86_64-*" { $arch = "x64" }
    "aarch64-*" { $arch = "arm64" }
    default { throw "Unsupported target: $target" }
}

$name = "qBit-$Version-windows-$arch"
$dist = "dist"
$staging = Join-Path $dist $name
$zip = Join-Path $dist "$name.zip"

Write-Host "Building qBit $Version for $target"
cargo build --release --locked --target $target
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$targetDir = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory

Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $staging, $zip
New-Item -ItemType Directory -Force $staging | Out-Null
Copy-Item (Join-Path $targetDir "$target/release/qbit.exe") $staging

Compress-Archive -Path $staging -DestinationPath $zip
Remove-Item -Recurse -Force $staging

Write-Host "Built:"
Get-Item $zip | Format-Table Name, Length
