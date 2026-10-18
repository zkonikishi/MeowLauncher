$ErrorActionPreference = 'Stop'
$suite = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $suite 'target\edition-process-check'
New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
$source = Join-Path $suite 'target\release\meow-launcher.exe'
$modernPath = Join-Path $testRoot 'MeowLauncher Modern.exe'
$retroPath = Join-Path $testRoot 'MeowLauncher Retro.exe'
Copy-Item -LiteralPath $source -Destination $modernPath
Copy-Item -LiteralPath $source -Destination $retroPath
$owned = @()
try {
    $modern = Start-Process -FilePath $modernPath -WindowStyle Hidden -PassThru
    $owned += $modern
    Start-Sleep -Seconds 3
    $retro = Start-Process -FilePath $retroPath -WindowStyle Hidden -PassThru
    $owned += $retro
    Start-Sleep -Seconds 3
    $modern.Refresh(); $retro.Refresh()
    if ($modern.HasExited -or $retro.HasExited) { throw 'An edition exited instead of remaining independent.' }
    $duplicate = Start-Process -FilePath $retroPath -WindowStyle Hidden -PassThru
    $owned += $duplicate
    if (-not $duplicate.WaitForExit(10000)) { throw 'Duplicate Retro instance did not exit.' }
    $modern.Refresh(); $retro.Refresh()
    if ($modern.HasExited -or $retro.HasExited) { throw 'An original edition exited after duplicate launch.' }
    $report = [ordered]@{
        passed = $true
        scope = 'Native process lifecycle; no login or game launch'
        modernTitle = $modern.MainWindowTitle
        retroTitle = $retro.MainWindowTitle
        independentProcesses = $true
        duplicateRetroExited = $true
    }
    $report | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $suite 'docs\qa\edition-process-check.json') -Encoding utf8
    $report | ConvertTo-Json
} finally {
    foreach ($process in $owned) {
        $process.Refresh()
        if (-not $process.HasExited) { Stop-Process -Id $process.Id }
    }
}
