$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$targetPid = [uint32]$env:MEOW_OBSERVER_PID
$after = [DateTimeOffset]::FromUnixTimeSeconds([long]$env:MEOW_OBSERVER_AFTER).LocalDateTime
$faults = @()
$attempts = 0
for ($attempt = 0; $attempt -lt 5; $attempt++) {
    $attempts++
    $events = @(Get-WinEvent -FilterHashtable @{LogName='Application'; Id=1000; StartTime=$after} -MaxEvents 64 -ErrorAction SilentlyContinue)
    foreach ($event in $events) {
        [xml]$xml = $event.ToXml()
        $fields = @{}
        foreach ($field in $xml.Event.EventData.Data) { $fields[[string]$field.Name] = [string]$field.'#text' }
        $eventPid = $fields['ProcessId']
        if (!$eventPid -or $eventPid -notmatch '^(0x[0-9a-fA-F]+|[0-9]+)$') { continue }
        $numericPid = if ($eventPid.StartsWith('0x')) { [Convert]::ToUInt32($eventPid.Substring(2),16) } else { [uint32]$eventPid }
        if ($numericPid -ne $targetPid -or $fields['AppName'] -ine 'DFO.exe') { continue }
        $module = [IO.Path]::GetFileName($fields['ModuleName'])
        $code = $fields['ExceptionCode']; $offset = $fields['FaultingOffset']
        if ($module -notmatch '^[A-Za-z0-9_.-]{1,128}$' -or $code -notmatch '^(0x)?[A-Fa-f0-9]{1,16}$' -or $offset -notmatch '^(0x)?[A-Fa-f0-9]{1,16}$') { continue }
        $faults += @{timeUtc=$event.TimeCreated.ToUniversalTime().ToString('o'); module=$module; exceptionCode=$code; moduleOffset=$offset}
        if ($faults.Count -ge 4) { break }
    }
    if ($faults.Count -gt 0) { break }
    if ($attempt -lt 4) { Start-Sleep -Milliseconds 1000 }
}
@{attempts=$attempts;faults=@($faults)} | ConvertTo-Json -Depth 4 -Compress
