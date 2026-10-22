# Download of the whisper.cpp model for the real ASR engine (opt-in).
#
# RF-36 / ADR-08: no network connection from the application itself.
# The download is **an explicit user action** (this script),
# never a behavior of the installed product. Afterwards, Voxli runs
# strictly offline.
#
# Run: powershell -ExecutionPolicy Bypass -File scripts/fetch-model.ps1
#   -Model ggml-base-q5_1.bin   (or tiny / small ...)

param(
    [string] $Model = 'ggml-base-q5_1.bin',
    [string] $Source = 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/'
)

$ErrorActionPreference = 'Stop'

$dest_dir = Join-Path $env:APPDATA 'Voxli\models'
$dest = Join-Path $dest_dir $Model

Write-Host ""
Write-Host "Modèle   : $Model"
Write-Host "Source   : $Source$Model"
Write-Host "Après    : $dest"
Write-Host ""
Write-Host "Aucun autre accès réseau n'est effectué par Voxli pendant l'usage." -ForegroundColor Yellow
$confirm = Read-Host 'Continuer le téléchargement ? (o/N)'
if ($confirm -notmatch '^[oO]') {
    Write-Host 'Abandon — aucune connexion.'
    exit 0
}

New-Item -ItemType Directory -Force -Path $dest_dir | Out-Null

$tmp = "$dest.part"
$ProgressPreference = 'SilentlyContinue'
Write-Host 'Téléchargement (peut prendre plusieurs minutes)...'
Invoke-WebRequest -Uri "$Source$Model" -OutFile $tmp -UseBasicParsing
Move-Item -Force $tmp $dest

$hash = (Get-FileHash -Algorithm SHA256 $dest).Hash.ToLower()
Write-Host ''
Write-Host ('SHA-256 : {0}' -f $hash)
Write-Host ('Taille  : {0} Mo' -f [math]::Round((Get-Item $dest).Length / 1MB, 1))
Write-Host ''
Write-Host 'Fait. Relancez Voxli compilé avec la fonctionnalité `real-asr` pour utiliser le moteur réel.'
