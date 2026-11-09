# 下班倒计时

常驻 Windows 桌面的倒计时挂件。Rust + Win32 + Direct3D 11，没有 .NET，也没有 WebView。

海洋着色器铺在圆角卡片后面，可以拖动、收成一条胶囊，也可以贴着工作区的外沿藏起来，只留大约 8 像素。鼠标停在露出的边上会滑出来，离开后再收回去。上班 9:00，下班 18:00。

## 构建

需要 `x86_64-pc-windows-gnu` 工具链。链接器写在 [`.cargo/config.toml`](.cargo/config.toml)，指向本机的 MinGW。

```powershell
cargo +stable-x86_64-pc-windows-gnu build --release
.\target\release\offwork-countdown.exe
```

## 海洋着色器

[`shader-src/Image.txt`](shader-src/Image.txt) 是 Shadertoy 的 Image pass。[`tools/convert-shader`](tools/convert-shader) 把它生成成 [`src/ocean.hlsl`](src/ocean.hlsl)。不要手改生成结果。

```powershell
cargo +stable-x86_64-pc-windows-gnu run --release --manifest-path tools/convert-shader/Cargo.toml
```

两张采样贴图是 [`assets/Channel0.png`](assets/Channel0.png) 和 [`assets/Channel1.jpg`](assets/Channel1.jpg)，编译进可执行文件。
