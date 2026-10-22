# Generation of the Voxli icons (NSIS bundle + tray + web).
#
# Input   : assets/logo.png (square 1080×1080 canvas).
# Outputs : assets/logo.ico, apps/desktop/src-tauri/icons/icon.ico (ICO with sizes
#           16/24/32/48/64/256 encoded as BMP 32bpp + AND mask — the canonical format
#           that rc.exe (tauri-build) and the Windows shell decode identically),
#           plus icons/32x32.png, 128x128.png and 256x256.png.
#
# WARNING: do not regenerate icon.ico via "pnpm tauri icon": it writes
# PNG-compressed entries that rc.exe flattens into a single image inside the exe (bugs
# observed in M1: window icon = a zoomed fragment). Go back through this script.
#
# Run: powershell -ExecutionPolicy Bypass -File scripts/make-icons.ps1

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$srcPng = Join-Path $root 'assets/logo.png'
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

# Builds a 32bpp ICO inner BMP (BITMAPINFOHEADER + BGRA pixels bottom->up +
# null AND mask: the alpha channel handles transparency).
function New-IconImage([System.Drawing.Image]$srcImg, [int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($srcImg, [System.Drawing.Rectangle]::new(0, 0, $size, $size))
    $g.Dispose()

    $rect = [System.Drawing.Rectangle]::new(0, 0, $size, $size)
    $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $stride = $data.Stride
    $rowBytes = $size * 4
    $xor = New-Object byte[] ($size * $rowBytes)
    $row = New-Object byte[] $rowBytes
    $o = 0
    for ($y = $size - 1; $y -ge 0; $y--) {  # ICO: first byte = bottom row
        [Runtime.InteropServices.Marshal]::Copy([IntPtr]($data.Scan0.ToInt64() + $y * $stride), $row, 0, $rowBytes)
        [Array]::Copy($row, 0, $xor, $o, $rowBytes)
        $o += $rowBytes
    }
    $bmp.UnlockBits($data)
    $bmp.Dispose()

    $maskStride = ((($size + 31)) -shr 5) -shl 2
    $and = New-Object byte[] ($maskStride * $size)

    $bih = New-Object byte[] 40
    [BitConverter]::GetBytes([int]40).CopyTo($bih, 0)          # biSize
    [BitConverter]::GetBytes([int]$size).CopyTo($bih, 4)       # biWidth
    [BitConverter]::GetBytes([int]($size * 2)).CopyTo($bih, 8) # biHeight (XOR+AND)
    [BitConverter]::GetBytes([uint16]1).CopyTo($bih, 12)       # biPlanes
    [BitConverter]::GetBytes([uint16]32).CopyTo($bih, 14)      # biBitCount (BI_RGB)

    $out = New-Object byte[] (40 + $xor.Length + $and.Length)
    [Array]::Copy($bih, 0, $out, 0, 40)
    [Array]::Copy($xor, 0, $out, 40, $xor.Length)
    [Array]::Copy($and, 0, $out, 40 + $xor.Length, $and.Length)
    return ,$out
}

function Write-Ico([string]$dest, [int[]]$sizes, [byte[][]]$images) {
    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter($ms)
    $count = $sizes.Count
    $bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$count)
    $offset = 6 + 16 * $count
    for ($i = 0; $i -lt $count; $i++) {
        $s = $sizes[$i]
        $bb = $(if ($s -ge 256) { [byte]0 } else { [byte]$s })
        $bw.Write($bb); $bw.Write($bb)                       # bWidth, bHeight (0 = 256)
        $bw.Write([byte]0); $bw.Write([byte]0)               # bColorCount, bReserved
        $bw.Write([uint16]1); $bw.Write([uint16]32)          # wPlanes, wBitCount
        $bw.Write([uint32]$images[$i].Length)                # dwBytesInRes
        $bw.Write([uint32]$offset)                           # dwImageOffset
        $offset += $images[$i].Length
    }
    foreach ($img in $images) { $bw.Write($img) }
    $bw.Flush()
    [IO.File]::WriteAllBytes($dest, $ms.ToArray())
    $bw.Dispose(); $ms.Dispose()
    Write-Host ("  ico : {0} ({1} octets, {2} images)" -f $dest, $offset, $count)
}

Write-Host 'Generation des icones Voxli (ICO multi-tailles BMP)...'
$sizes = [int[]](16, 24, 32, 48, 64, 256)
$srcImg = [System.Drawing.Image]::FromFile($srcPng)
$images = foreach ($s in $sizes) { New-IconImage $srcImg $s }
$images = [byte[][]]$images
$srcImg.Dispose()

Write-Ico (Join-Path $root 'assets/logo.ico') $sizes $images
Write-Ico (Join-Path $iconDir 'icon.ico') $sizes $images

Resize-Png $srcPng (Join-Path $iconDir '32x32.png') 32
Resize-Png $srcPng (Join-Path $iconDir '128x128.png') 128
Resize-Png $srcPng (Join-Path $iconDir '256x256.png') 256
