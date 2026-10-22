# Memory measurement (docs/06 §4: resting memory target under 210 MB).
#
# Samples the working set + private memory of the Voxli process
# during an observation window, printing min/avg/max.
#
# Run: powershell -ExecutionPolicy Bypass -File scripts/measure-memory.ps1 -Seconds 60

param(
    [int] $Seconds = 60,
    [string] $ProcessName = 'Voxli'
)

$ErrorActionPreference = 'SilentlyContinue'

Write-Host "Observation de '$ProcessName' pendant $Seconds s —"
Write-Host "faites quelques dictées pendant ce temps (repos puis usage)."

$points = New-Object System.Collections.Generic.List[double]
$end = (Get-Date).AddSeconds($Seconds)

while ((Get-Date) -lt $end) {
    $p = Get-Process -Name $ProcessName | Select-Object -First 1
    if ($p) {
        # Working set + private memory: the two halves of the WebView2 budget.
        $mb = ($p.WorkingSet64 + $p.PrivateMemorySize64) / 1MB
        $points.Add($mb) | Out-Null
        Write-Output (('{0:HH:mm:ss}  {1,7:F1} Mo' -f (Get-Date), $mb))
    } else {
        Write-Output ('{0:HH:mm:ss}  processus introuvable — lancez Voxli d''abord' -f (Get-Date))
        Start-Sleep 2
    }
    Start-Sleep -Milliseconds 1000
}

if ($points.Count -gt 0) {
    $sorted = $points | Sort-Object
    $p95 = $sorted[[int][math]::Floor($sorted.Count * 0.95) - 1]
    Write-Host ''
    Write-Output ('échantillons : {0}' -f $points.Count)
    Write-Output ('min           : {0:F1} Mo' -f ($points | Measure-Object -Minimum).Minimum)
    Write-Output ('moyenne       : {0:F1} Mo' -f ($points | Measure-Object -Average).Average)
    Write-Output ('max           : {0:F1} Mo' -f ($points | Measure-Object -Maximum).Maximum)
    Write-Output ('p95           : {0:F1} Mo' -f $p95)
    Write-Host ''
    Write-Host ('Cible RF-07/ADR-09 : repos < 210 Mo (hors inférence).')
}