<#
.SYNOPSIS
  Build Zenda's DeckCraft for Work and pack the portable zip Work downloads.
.DESCRIPTION
  Builds deckcraft.exe (with the zenda feature) and deckcraft-cli.exe, and writes
  dist\zenda\deckcraft-<version>-z<n>-windows-x64-portable.zip. Inside, the folder keeps upstream's
  name (deckcraft-<version>-windows-x64-portable), so Work's manifest keeps the same `exe` path.
  Prints the zip's size and SHA-256 for app/manifest.json in zenda-work.
.EXAMPLE
  pwsh zenda/package.ps1 -Z 1
#>
param([Parameter(Mandatory)] [int] $Z, [switch] $SkipBuild)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Version = (Select-String -Path (Join-Path $Root 'Cargo.toml') -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $env:DECKCRAFT_BUILD_SHA) { $env:GRIDCRAFT_BUILD_SHA = (git -C $Root rev-parse HEAD) }
if (-not $SkipBuild) {
  $env:RUSTFLAGS = '-C target-feature=+crt-static'
  cargo build --release --locked -p deckcraft -p deckcraft-cli --features deckcraft/zenda
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
}
$Target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
$Dist = Join-Path $Root 'dist\zenda'
$Folder = "deckcraft-$Version-windows-x64-portable"
$Stage = Join-Path $Dist $Folder
if (Test-Path $Stage) { throw "Staging already exists: $Stage; use a clean checkout" }
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item (Join-Path $Target 'release\deckcraft.exe'), (Join-Path $Target 'release\deckcraft-cli.exe') $Stage
foreach ($f in 'LICENSE-APACHE', 'LICENSE-MIT', 'NOTICE', 'ATTRIBUTION.md') { Copy-Item (Join-Path $Root $f) $Stage }

# Builds made with craft-fonts embed its fonts: ship their licences, as upstream's package.ps1 does.
if ($env:CRAFT_FONTS_DIR) {
  foreach ($ofl in Get-ChildItem -Path (Join-Path $env:CRAFT_FONTS_DIR 'fonts\*\OFL.txt') -ErrorAction SilentlyContinue) {
    Copy-Item $ofl.FullName (Join-Path $Stage "OFL-$($ofl.Directory.Name).txt")
  }
}
$Zip = Join-Path $Dist "deckcraft-$Version-z$Z-windows-x64-portable.zip"
Compress-Archive -Path $Stage -DestinationPath $Zip
$item = Get-Item $Zip
$sha = (Get-FileHash -Algorithm SHA256 $Zip).Hash.ToLower()
Write-Output "zip:    $($item.Name)"
Write-Output "size:   $($item.Length)"
Write-Output "sha256: $sha"
Write-Output "exe:    $Folder/deckcraft.exe"
