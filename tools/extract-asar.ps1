# Extract an Electron app.asar archive (no external tools required).
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File extract-asar.ps1 -AsarPath <app.asar> -OutDir <dir> [-Quiet]
# Format (electron asar): [u32 4][u32 headerSize] + header pickle:
#   [u32 payloadSize][u32 jsonSize][json bytes][pad to 4] then the data section.
# File offsets in the JSON header are relative to the data section start
# (dataStart = 8 + headerSize).
param(
    [Parameter(Mandatory = $true)][string]$AsarPath,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [switch]$Quiet
)
$ErrorActionPreference = 'Stop'
$asar = (Resolve-Path -LiteralPath $AsarPath).Path
$unpackedRoot = $asar + '.unpacked'
$fs = [System.IO.File]::OpenRead($asar)
try {
    $br = New-Object System.IO.BinaryReader($fs)
    $pad = $br.ReadUInt32()
    $headerSize = $br.ReadUInt32()
    if ($headerSize -le 0 -or $headerSize -gt 256MB) { throw "bad asar header size: $headerSize" }
    # Header pickle: size of payload, then size of the JSON string, then JSON.
    $payloadSize = $br.ReadUInt32()
    $jsonSize = $br.ReadUInt32()
    if ($jsonSize -le 0 -or $jsonSize -gt $headerSize) { throw "bad asar json size: $jsonSize" }
    $headerBytes = $br.ReadBytes([int]$jsonSize)
    $json = [System.Text.Encoding]::UTF8.GetString($headerBytes)
    $header = $json | ConvertFrom-Json
    $dataStart = [int64](8 + $headerSize)
    if ($dataStart % 4 -ne 0) { $dataStart = $dataStart + (4 - ($dataStart % 4)) }
    if (-not $Quiet) { Write-Host "asar=$asar headerSize=$headerSize dataStart=$dataStart" }

    if (-not (Test-Path -LiteralPath $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
    $outRoot = (Resolve-Path -LiteralPath $OutDir).Path
    $script:fileCount = 0
    $script:unpackedCount = 0
    $script:skipped = New-Object System.Collections.Generic.List[string]

    function Write-Entry([string]$rel, [int64]$offset, [int64]$size) {
        $dest = Join-Path $outRoot $rel
        $destDir = [System.IO.Path]::GetDirectoryName($dest)
        if ($destDir -and -not (Test-Path -LiteralPath $destDir)) { New-Item -ItemType Directory -Path $destDir -Force | Out-Null }
        $fs.Position = $dataStart + $offset
        $buf = New-Object byte[] $size
        $read = 0
        while ($read -lt $size) {
            $n = $fs.Read($buf, $read, [int]($size - $read))
            if ($n -le 0) { throw "unexpected EOF at $rel" }
            $read += $n
        }
        [System.IO.File]::WriteAllBytes($dest, $buf)
        $script:fileCount++
    }

    function Walk($node, [string]$rel) {
        foreach ($p in $node.files.PSObject.Properties) {
            $name = $p.Name
            $entry = $p.Value
            $childRel = if ($rel) { $rel + '/' + $name } else { $name }
            $isDir = $null -ne $entry.PSObject.Properties['files']
            if ($isDir) { Walk $entry $childRel; continue }
            if ($entry.unpacked -eq $true) {
                # File physically lives in <app.asar>.unpacked/<childRel>.
                $src = Join-Path $unpackedRoot ($childRel -replace '/', [System.IO.Path]::DirectorySeparatorChar)
                $dest = Join-Path $outRoot ($childRel -replace '/', [System.IO.Path]::DirectorySeparatorChar)
                if (Test-Path -LiteralPath $src) {
                    $destDir = [System.IO.Path]::GetDirectoryName($dest)
                    if ($destDir -and -not (Test-Path -LiteralPath $destDir)) { New-Item -ItemType Directory -Path $destDir -Force | Out-Null }
                    Copy-Item -LiteralPath $src -Destination $dest -Force
                    $script:unpackedCount++
                } else {
                    $script:skipped.Add($childRel)
                }
                continue
            }
            Write-Entry $childRel ([int64]$entry.offset) ([int64]$entry.size)
        }
    }

    Walk $header ''
    if (-not $Quiet) {
        Write-Host "extracted=$($script:fileCount) unpackedCopied=$($script:unpackedCount) skipped=$($script:skipped.Count)"
        if ($script:skipped.Count -gt 0) { Write-Host ("skipped: " + ($script:skipped -join ', ')) }
    }
    $summary = [ordered]@{
        asar        = $asar
        outDir      = $outRoot
        dataStart   = $dataStart
        extracted   = $script:fileCount
        unpacked    = $script:unpackedCount
        skipped     = $script:skipped.ToArray()
    }
    $summary | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $outRoot '_asar-extract-summary.json') -Encoding UTF8
}
finally {
    $fs.Dispose()
}
