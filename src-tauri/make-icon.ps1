Add-Type -AssemblyName System.Drawing
$sz = 1024
$bmp = New-Object System.Drawing.Bitmap($sz, $sz)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$bg = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 18, 22, 38))
$g.FillRectangle($bg, 0, 0, $sz, $sz)
$accent = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 94, 129, 255))
$pen = New-Object System.Drawing.Pen($accent, 70)
$g.DrawEllipse($pen, 230, 230, 564, 564)
$g.FillEllipse($accent, 462, 462, 100, 100)
$out = Join-Path (Get-Location) "src-tauri\icon-source.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()
if (Test-Path $out) { "ICON_SRC_OK size=" + (Get-Item $out).Length } else { "ICON_SRC_FAIL" }
