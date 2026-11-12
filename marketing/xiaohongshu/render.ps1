$ErrorActionPreference = "Stop"

$chrome = "C:\Program Files\Google\Chrome\Application\chrome.exe"
if (-not (Test-Path $chrome)) {
    throw "Chrome not found at $chrome"
}

$images = Join-Path $PSScriptRoot "images"
New-Item -ItemType Directory -Force $images | Out-Null

foreach ($n in @("01", "02", "03", "04", "05")) {
    $profile = Join-Path $env:TEMP "offwork-xhs-$n"
    $out = Join-Path $images "$n.png"
    $url = "file:///" + (Join-Path $PSScriptRoot "render/$n.html").Replace("\", "/")
    & $chrome --headless=new --no-sandbox --disable-gpu --hide-scrollbars `
        --no-first-run --no-default-browser-check `
        --force-device-scale-factor=1 `
        --user-data-dir=$profile `
        --window-size=1080,1440 `
        "--screenshot=$out" `
        $url 2>$null
    Start-Sleep -Milliseconds 900
    Write-Output "rendered $n.png"
}
