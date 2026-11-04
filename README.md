# 下班倒计时（OffWorkCountdown）

一个常驻 Windows 桌面的轻量倒计时挂件：显示距离下班还有多久，支持四边贴边自动隐藏、拖拽、折叠和即改即生效的设置。原生 WPF，单文件 exe，不依赖 Electron / WebView2。

![下班倒计时](preview.png)

## 功能

- 实时下班倒计时、进度条和百分比；到点显示“已下班 / 今天辛苦了”，第二天自动回到倒计时。
- **四边贴边隐藏**：拖到屏幕任意一边附近，只留 6px 把手；鼠标碰到把手滑出，移开自动缩回。上下边优先；底部可贴到屏幕最底，卡片其余部分不会盖住任务栏。
- 拖拽移动并记忆位置；点右上角收起成小胶囊，再点展开。
- **设置即改即生效**，没有保存/取消：
  - 上班/下班时间支持 `1830`、`18:30` 这类输入，也可以用 `+/-` 每次调 30 分钟；
  - 下班提醒音（默认关闭）；
  - 开机自启；
  - 中文 / English 分段切换。
- 下班时如果正贴边隐藏，会自动展开 5 秒提醒下班。
- 托盘菜单、隐藏/显示、退出、单实例；无任务栏按钮。
- 无尺寸/透明度动画，切换时不会闪字。

## 系统要求

- **Windows 10 / Windows 11**（64 位或 32 位；系统自带 .NET Framework 4.8 即可）。
- 不需要安装 .NET / Visual Studio / WebView2；不需要管理员权限。
- 单文件 exe，配置保存在 `%APPDATA%\OffWorkCountdown\settings.json`。

## 下载

到 [Releases](https://github.com/Guyu23/offwork-countdown/releases/latest) 下载 `OffWorkCountdown.exe`，双击运行。

- exe 未签名；首次运行如果被 SmartScreen 拦住，点“更多信息 → 仍要运行”。

## 截图

| 主界面 | 设置 |
| --- | --- |
| ![主界面](preview.png) | ![设置](preview-settings.png) |

![收起状态](preview-collapsed.png)

## 字体与许可

- 中文：使用系统自带的 **Microsoft YaHei**，不额外嵌入中文字体。
- 英文和数字：**Maple Mono**（SIL Open Font License 1.1，已内嵌），许可文本见 [`assets/OFL-MapleMono.txt`](assets/OFL-MapleMono.txt)。

## 构建

需要 Windows 10/11 自带的 .NET Framework 4.8 编译器。

```powershell
.\build.ps1
```

构建脚本使用 `C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe` 直接编译，不需要 Visual Studio 或 .NET SDK。源码在 [`src/Program.cs`](src/Program.cs)。

当前版本：v1.1.0
