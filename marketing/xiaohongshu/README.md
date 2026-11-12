# 小红书宣传图

5 张 1080 × 1440 的图集，配一份可直接发布的小红书文案。

## 文件

- `images/01.png` 封面：离下班，还有一点好时光。
- `images/02.png` 功能：它只做一件事。
- `images/03.png` 系列：四种风景。
- `images/04.png` 情绪：到点了。
- `images/05.png` 行动：扫码去发布页。
- `copy.md` 主推文案、备选文案、评论区回复。
- `analysis.md` / `outline.md` 内容分析与图集结构。
- `prompts/` 每张图的文字、素材和版式 brief。
- `qr.png` 指向 `https://guyu23.github.io/offwork-countdown/` 的二维码。
- `render/` 可重新渲染的 HTML/CSS 源文件。

## 设计

深海墨蓝、奶油白、落日橘，和发布页保持同一套视觉。标题使用 Noto Serif SC，正文使用 Noto Sans SC。所有界面都是真实 2x 截图，没有 AI 重绘 UI 或文字。

## 重新渲染

需要 Chrome。在仓库根目录执行：

```powershell
.\marketing\xiaohongshu\render.ps1
```

或手动渲染单张：

```powershell
& "C:\Program Files\Google\Chrome\Application\chrome.exe" `
  --headless=new --disable-gpu --hide-scrollbars `
  --user-data-dir="$env:TEMP\offwork-xhs" `
  --window-size=1080,1440 `
  --screenshot="marketing\xiaohongshu\images\01.png" `
  "file:///$PWD/marketing/xiaohongshu/render/01.html"
```

改标题或正文时，同时改 `render/0N.html` 和 `copy.md`，然后重新渲染。
