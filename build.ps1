# 使用 Windows 自带的 C# 编译器构建，不依赖 Visual Studio / .NET SDK。
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path $root 'src\Program.cs'
$manifest = Join-Path $root 'app.manifest'
$dist = Join-Path $root 'dist'
$exe = Join-Path $dist 'OffWorkCountdown.exe'

$framework = 'C:\Windows\Microsoft.NET\Framework64\v4.0.30319'
$csc = Join-Path $framework 'csc.exe'
if (-not (Test-Path -LiteralPath $csc)) {
    throw "找不到 Windows 自带的 C# 编译器：$csc"
}

$references = @(
    (Join-Path $framework 'WPF\PresentationFramework.dll'),
    (Join-Path $framework 'WPF\PresentationCore.dll'),
    (Join-Path $framework 'WPF\WindowsBase.dll'),
    (Join-Path $framework 'System.Xaml.dll'),
    (Join-Path $framework 'System.dll'),
    (Join-Path $framework 'System.Core.dll'),
    (Join-Path $framework 'System.Drawing.dll'),
    (Join-Path $framework 'System.Windows.Forms.dll'),
    (Join-Path $framework 'System.Runtime.Serialization.dll'),
    (Join-Path $framework 'System.Xml.dll')
)

foreach ($reference in $references) {
    if (-not (Test-Path -LiteralPath $reference)) {
        throw "缺少构建所需程序集：$reference"
    }
}

New-Item -ItemType Directory -Force -Path $dist | Out-Null

# 生成一个简单的蓝底时钟图标，仅用于 exe 文件图标
$assets = Join-Path $root 'assets'
$icon = Join-Path $assets 'OffWorkCountdown.ico'
New-Item -ItemType Directory -Force -Path $assets | Out-Null
try {
    Add-Type -AssemblyName System.Drawing
    $bitmap = New-Object System.Drawing.Bitmap 64, 64
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.Clear([System.Drawing.Color]::Transparent)
    $fill = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 5, 112, 238))
    $graphics.FillEllipse($fill, 2, 2, 60, 60)
    $pen = New-Object System.Drawing.Pen([System.Drawing.Color]::White, 4.5)
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $graphics.DrawEllipse($pen, 17, 17, 30, 30)
    $graphics.DrawLine($pen, 32, 22, 32, 34)
    $graphics.DrawLine($pen, 32, 34, 40, 39)
    $iconHandle = $bitmap.GetHicon()
    $drawingIcon = [System.Drawing.Icon]::FromHandle($iconHandle)
    $stream = [System.IO.File]::Create($icon)
    $drawingIcon.Save($stream)
    $stream.Dispose()
    $drawingIcon.Dispose()
    $pen.Dispose()
    $fill.Dispose()
    $graphics.Dispose()
    $bitmap.Dispose()
}
catch {
    Write-Warning "图标生成失败，将使用默认 exe 图标：$($_.Exception.Message)"
    $icon = $null
}

$cscArgs = @(
    '/nologo',
    '/target:winexe',
    '/platform:anycpu',
    '/optimize+',
    '/debug-',
    '/langversion:5',
    '/codepage:65001',
    '/main:OffWorkCountdown.Program',
    "/win32manifest:$manifest",
    "/out:$exe"
)
if ($icon -and (Test-Path -LiteralPath $icon)) {
    $cscArgs += "/win32icon:$icon"
}
foreach ($reference in $references) {
    $cscArgs += "/r:$reference"
}
$cscArgs += $src

& $csc $cscArgs
if ($LASTEXITCODE -ne 0) {
    throw "编译失败，退出码 $LASTEXITCODE"
}
if (-not (Test-Path -LiteralPath $exe)) {
    throw "编译结束但没有找到 $exe"
}

$file = Get-Item -LiteralPath $exe
Write-Host ("构建完成：" + $file.FullName)
Write-Host ("文件大小：" + [math]::Round($file.Length / 1KB, 1) + " KB")
