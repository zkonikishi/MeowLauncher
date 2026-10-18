param([switch]$SkipTests, [switch]$LegacyOnline, [string]$ReleaseName = '0.7.0-review', [ValidateSet('meowopia-custom','sponsor')][string]$Edition = 'sponsor')
$ErrorActionPreference = 'Stop'
$suiteRoot = Split-Path -Parent $PSScriptRoot
$previousEdition = $env:MEOW_LAUNCHER_EDITION
if ($LegacyOnline -and $Edition -eq 'sponsor') { throw 'Generic edition cannot use the Meowopia legacy channel.' }
$env:MEOW_LAUNCHER_EDITION = $Edition
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'cargo not found in PATH. Install the Rust toolchain from https://rustup.rs and reopen the shell.'
}
Push-Location $suiteRoot
try {
    if (-not $SkipTests) {
        cargo test -p meow-launcher-core --locked
        if ($LASTEXITCODE -ne 0) { throw 'Core tests failed.' }
        node --test tools/model.test.mjs
        if ($LASTEXITCODE -ne 0) { throw 'UI model tests failed.' }
    }
    if ($ReleaseName -notmatch '^[A-Za-z0-9._-]+$') { throw 'Invalid release directory name.' }
    $releaseDirectory = Join-Path (Join-Path $suiteRoot 'releases') $ReleaseName
    New-Item -ItemType Directory -Path $releaseDirectory -Force | Out-Null
    if ($LegacyOnline) { cargo build -p meow-launcher --release --features legacy-online --locked }
    else { cargo build -p meow-launcher --release --locked }
    if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }
    $exeName = if ($Edition -eq 'sponsor') { 'MeowLauncher.exe' } else { 'MeowLauncher Max.exe' }
    Copy-Item -LiteralPath 'target\release\meow-launcher.exe' -Destination (Join-Path $releaseDirectory $exeName)
    if ($Edition -eq 'sponsor') {
        Copy-Item -LiteralPath 'docs\普通版使用说明.txt' -Destination (Join-Path $releaseDirectory '使用说明.txt')
    } elseif (Test-Path 'docs\使用说明.txt') {
        Copy-Item -LiteralPath 'docs\使用说明.txt' -Destination $releaseDirectory
    } else {
        Write-Warning 'Custom-edition manual docs\使用说明.txt not present in this repository; manual copy skipped.'
    }
    Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $releaseDirectory $exeName) | Format-List
    Write-Output "Build ready: $releaseDirectory"
} finally { $env:MEOW_LAUNCHER_EDITION = $previousEdition; Pop-Location }
