# Draws the icons of the built-in commands (src/commands.rs) into multi-size
# .ico files (PNG entries) next to this script. Any other .ico works too.
# Run from anywhere: powershell -File assets\commands\make-icons.ps1
Add-Type -AssemblyName System.Drawing

$sizes = 16, 24, 32, 48, 64, 128, 256

function Write-Ico($name, [scriptblock]$draw) {
    $images = foreach ($size in $sizes) {
        $bmp = New-Object System.Drawing.Bitmap $size, $size
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.SmoothingMode = 'AntiAlias'
        # Draw in 256x256 units.
        $g.ScaleTransform($size / 256.0, $size / 256.0)
        & $draw $g
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
    [System.IO.File]::WriteAllBytes((Join-Path $PSScriptRoot "$name.ico"), $out.ToArray())
}

function Brush($hex) {
    New-Object System.Drawing.SolidBrush ([System.Drawing.ColorTranslator]::FromHtml($hex))
}

# Red rounded rectangle with a white play triangle.
Write-Ico 'youtube' {
    param($g)
    $x, $y, $w, $h, $r = 8, 52, 240, 152, 44
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc($x, $y, 2 * $r, 2 * $r, 180, 90)
    $path.AddArc($x + $w - 2 * $r, $y, 2 * $r, 2 * $r, 270, 90)
    $path.AddArc($x + $w - 2 * $r, $y + $h - 2 * $r, 2 * $r, 2 * $r, 0, 90)
    $path.AddArc($x, $y + $h - 2 * $r, 2 * $r, 2 * $r, 90, 90)
    $path.CloseFigure()
    $g.FillPath((Brush '#FF0033'), $path)
    $tri = [System.Drawing.PointF[]]@(
        (New-Object System.Drawing.PointF 102, 86),
        (New-Object System.Drawing.PointF 172, 128),
        (New-Object System.Drawing.PointF 102, 170))
    $g.FillPolygon((Brush '#FFFFFF'), $tri)
}

# Terracotta spark: round-capped rays of alternating length.
Write-Ico 'claude' {
    param($g)
    $color = [System.Drawing.ColorTranslator]::FromHtml('#D97757')
    $pen = New-Object System.Drawing.Pen $color, 24
    $pen.StartCap = 'Round'; $pen.EndCap = 'Round'
    $rays = 12
    for ($i = 0; $i -lt $rays; $i++) {
        $a = 2 * [math]::PI * $i / $rays + 0.2
        $len = if ($i % 2) { 82 } else { 108 }
        $g.DrawLine($pen, 128, 128, 128 + $len * [math]::Cos($a), 128 + $len * [math]::Sin($a))
    }
}
