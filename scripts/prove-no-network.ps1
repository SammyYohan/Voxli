# Proof of absence of outbound connection (RF-36, docs/06 §5.3).
#
# During the observation window, relays via Get-NetTCPConnection all the
# TCP sockets of the Voxli process. A single non-local outbound ESTABLISHED is a
# failure. The report dumped at the end serves as acceptance-test proof.
#
# Run:
#   start Voxli, then:
#   powershell -ExecutionPolicy Bypass -File scripts/prove-no-network.ps1 -Seconds 120

param(
    [int] $Seconds = 120,
    [string] $ProcessName = 'Voxli'
)

$ErrorActionPreference = 'Stop'

$procs = Get-Process -Name $ProcessName -ErrorAction Stop
$pids = $procs.Id
Write-Host ("Surveillance réseau de Voxli (PID {0}) pendant {1} s..." -f ($pids -join ', '), $Seconds)
Write-Host "Pendant ce temps : dicitez normalement, y compris une transcription complete."

$end = (Get-Date).AddSeconds($Seconds)
$violations = New-Object System.Collections.Generic.List[string]
$seen = 0

while ((Get-Date) -lt $end) {
    Start-Sleep -Milliseconds 500
    $conns = Get-NetTCPConnection -ErrorAction SilentlyContinue
    foreach ($c in $conns) {
        if ($pids -notcontains $c.OwningProcess) { continue }
        $seen++
        if ($c.State -eq 'Established') {
            $remote = $c.RemoteAddress
            # localhost: IPv4/IPv6 loopback and same machine
            $isLocal = $remote -in @('127.0.0.1', '::1')
            if (-not $isLocal) {
                $line = '{0:HH:mm:ss} ESTABLISHED vers {1}:{2} (port local {3})' -f (Get-Date), $remote, $c.RemotePort, $c.LocalPort
                $violations.Add($line) | Out-Null
                Write-Warning $line
            }
        }
    }
}

Write-Host ''
if ($violations.Count -eq 0) {
    Write-Host ("PREUVE : aucune connexion sortante établie détectée sur {0} points d'échantillon." -f ($seen -as [int])) -ForegroundColor Green
    $report = Join-Path (Get-Location) 'preuve-no-reseau.txt'
    @(
        "Preuve d'absence de connexion sortante — Voxli"
        "Date        : $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
        "Observation : $Seconds s autour du PID(s) $($pids -join ', ')"
        "Connexions potentielles observées : $(if ($seen) { $seen } else { 'aucune' })"
        "Violations : 0"
    ) | Set-Content $report
    Write-Host ("Rapport : {0}" -f $report)
    exit 0
} else {
    Write-Host ("Échec : {0} connexion(s) sortante(s) établie(s) détectée(s)." -f $violations.Count) -ForegroundColor Red
    $violations | Set-Content 'preuve-no-reseau-ECHUE.txt'
    exit 1
}