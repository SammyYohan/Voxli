# Génération des icônes Plume (favicon de bundle NSIS + tray).
#
# Utilise System.Drawing (GDI+) : aucune dépendance externe. Dessin simple :
# fond indigo arrondi + trait de plume blanc. Produit les trois fichiers
# référencés par tauri.conf.json (icons/icon.ico, 32x32.png, 128x128.png).
#
# Exécuter : powershell -ExecutionPolicy Bypass -File scripts/make-icons.ps1

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$dir = Join-Path $root 'apps/desktop/src-tauri/icons'
New-Item -ItemType Directory -Force -Path $dir | Out-Null

function New-RoundedRect([int] $width, [int] $height, [int] $radius) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = 2 * $radius
    $p.AddArc(0, 0, $d, $d, 180, 90)
    $p.AddArc($width - $d, 0, $d, $d, 270, 90)
    $p.AddArc($width - $d, $height - $d, $d, $d, 0, 90)
    $p.AddArc(0, $height - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

function New-PlumePng([int] $size, [string] $dest) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    # Fond arrondi indigo
    $rect = New-Object System.Drawing.Rectangle(0, 0, $size, $size)
    $round = New-RoundedRect $size $size ([int]($size * 0.22))
    $fill = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 62, 69, 138))
    $g.FillPath($fill, $round)

    # Plume : hampe diagonale + barbes
    $white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(245, 250, 250, 255))
    $penW = [float]($size * 0.085)
    $pen = New-Object System.Drawing.Pen([System.Drawing.Color]::White, $penW)
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round

    # fût : du haut-droit vers le bas-gauche
    $x1 = $size * 0.74; $y1 = $size * 0.22
    $x2 = $size * 0.38; $y2 = $size * 0.78
    $g.DrawLine($pen, [float]$x1, [float]$y1, [float]$x2, [float]$y2)

    # barbes paralleles
    $n = 3
    for ($i = 1; $i -le $n; $i++) {
        $t = $i / ($n + 1.0)
        $cx = $x1 + ($x2 - $x1) * $t
        $cy = $y1 + ($y2 - $y1) * $t
        $g.DrawLine($pen, [float]$cx, [float]$cy, [float]($cx + $size * 0.14), [float]($cy - $size * 0.10))
    }

    # pointe bas-gauche
    $penTip = New-Object System.Drawing.Pen([System.Drawing.Color]::White, [float]($size * 0.055))
    $penTip.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $penTip.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $g.DrawLine($penTip, [float]($size * 0.38), [float]($size * 0.78), [float]($size * 0.26), [float]($size * 0.86))

    $g.Dispose()
    $bmp.Save($dest, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host "  icone : $dest"
}

Write-Host 'Generation des icones Plume...'
New-PlumePng 32  (Join-Path $dir '32x32.png')
New-PlumePng 128 (Join-Path $dir '128x128.png')

# ICO multi-tailles (32 + 256) : conteneur ICO avec blobs PNG (Vista+).
function Write-Ico {
    param([string] $Png32, [string] $Png256Path, [string] $IcoPath)

    New-PlumePng 256 $Png256Path | Out-Null
    $b32 = [System.IO.File]::ReadAllBytes($Png32)
    $b256 = [System.IO.File]::ReadAllBytes($Png256Path)

    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter($ms)

    # ICONDIR : reserve=0, type=1 (icône), nombre=2
    $bw.Write([UInt16]0); $bw.Write([UInt16]1); $bw.Write([UInt16]2)

    $offset = 6 + 2 * 16

    $bw.Write([byte]32);  $bw.Write([byte]32)   # largeur, hauteur
    $bw.Write([byte]0);   $bw.Write([byte]0)    # palette, reserve
    $bw.Write([UInt16]1); $bw.Write([UInt16]32) # plan, bpp
    $bw.Write([UInt32]$b32.Length); $bw.Write([UInt32]$offset)
    $offset += $b32.Length

    $bw.Write([byte]0);   $bw.Write([byte]0)    # 256 -> 0
    $bw.Write([byte]0);   $bw.Write([byte]0)
    $bw.Write([UInt16]1); $bw.Write([UInt16]32)
    $bw.Write([UInt32]$b256.Length); $bw.Write([UInt32]$offset)

    $bw.Write($b32)
    $bw.Write($b256)
    $bw.Flush()
    [System.IO.File]::WriteAllBytes($IcoPath, $ms.ToArray())
    $bw.Dispose(); $ms.Dispose()
    Remove-Item -Force $Png256Path
    Write-Host "  icone : $IcoPath"
}

Write-Ico -Png32 (Join-Path $dir '32x32.png') `
          -Png256Path (Join-Path $dir '256x256.png') `
          -IcoPath (Join-Path $dir 'icon.ico')

Write-Host 'Icônes écrites.'