# Renders the bar from icon.svg into a multi-size icon.ico (PNG entries).
# Run from anywhere: powershell -File assets\make-icon.ps1
Add-Type -AssemblyName System.Drawing

$sizes = 16, 20, 24, 32, 48, 64, 256

$images = foreach ($size in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $k = $size / 256.0
    # Same shapes as icon.svg, in 256x256 units.
    $rect = { param($x, $y, $w, $h) New-Object System.Drawing.RectangleF ($x * $k), ($y * $k), ($w * $k), ($h * $k) }
    $fill = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(0x3c, 0x3c, 0x3c))
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(0x9a, 0x9a, 0x9a)), ([math]::Max(1.0, 8 * $k))
    $bar = & $rect 16 88 224 80
    $g.FillRectangle($fill, $bar)
    $g.DrawRectangle($pen, $bar.X, $bar.Y, $bar.Width, $bar.Height)
    $g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(0x4b, 0x73, 0xff))), (& $rect 40 112 32 32))
    $g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(0x9a, 0x9a, 0x9a))), (& $rect 84 112 32 32))
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    , $ms.ToArray()
}

$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $out
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dim = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32)
    $w.Write([uint32]$images[$i].Length); $w.Write([uint32]$offset)
    $offset += $images[$i].Length
}
foreach ($img in $images) { $w.Write($img) }
[System.IO.File]::WriteAllBytes((Join-Path $PSScriptRoot 'icon.ico'), $out.ToArray())
