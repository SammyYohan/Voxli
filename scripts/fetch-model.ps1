# Téléchargement du modèle whisper.cpp pour le moteur ASR réel (opt-in).
#
# RF-36 / ADR-08 : aucune connexion réseau par l'application elle-même.
# Le téléchargement est **une action explicite de l'utilisateur** (ce script),
# jamais un comportement du produit installé. Après coup, Voxli fonctionne
#strictement hors ligne.
#
# Exécuter : powershell -ExecutionPolicy Bypass -File scripts/fetch-model.ps1
#   -Model ggml-base-q5_1.bin   (ou tiny / small ...)

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
