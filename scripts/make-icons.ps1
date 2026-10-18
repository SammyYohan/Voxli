# Génération des icônes Voxli (bundle NSIS + tray + web).
#
# Utilise assets/logo.png et System.Drawing pour redimensionner les icônes.
# Produit les fichiers référencés par tauri.conf.json (icons/icon.ico, 32x32.png, 128x128.png).
#
# Exécuter : powershell -ExecutionPolicy Bypass -File scripts/make-icons.ps1

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$srcPng = Join-Path $root 'assets/logo.png'
$srcIco = Join-Path $root 'assets/logo.ico'
$iconDir = Join-Path $root 'apps/desktop/src-tauri/icons'

if (-not (Test-Path $srcPng)) {
    throw "Logo source introuvable : $srcPng"
}

New-Item -ItemType Directory -Force -Path $iconDir | Out-Null

function Resize-Png([string]$source, [string]$dest, [int]$size) {
    $srcImg = [System.Drawing.Image]::FromFile($source)
    $destBmp = New-Object System.Drawing.Bitmap($size, $size)
    $g = [System.Drawing.Graphics]::FromImage($destBmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($srcImg, 0, 0, $size, $size)
    $g.Dispose()
    $srcImg.Dispose()
    $destBmp.Save($dest, [System.Drawing.Imaging.ImageFormat]::Png)
    $destBmp.Dispose()
    Write-Host "  icone : $dest"
}

Write-Host 'Generation des icones Voxli...'
Resize-Png $srcPng (Join-Path $iconDir '32x32.png') 32
Resize-Png $srcPng (Join-Path $iconDir '128x128.png') 128
Resize-Png $srcPng (Join-Path $iconDir '256x256.png') 256

if (Test-Path $srcIco) {
    Copy-Item $srcIco (Join-Path $iconDir 'icon.ico') -Force
    Write-Host "  ico copie : $(Join-Path $iconDir 'icon.ico')"
}