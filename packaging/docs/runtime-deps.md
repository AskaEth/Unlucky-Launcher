# 每平台运行时依赖与软渲染兜底（M145 拆自 `packaging/README.md`）

本文承载原 `packaging/README.md` 的 §4、§5，**节号沿用原编号**（全仓 `packaging/README.md §N` 这类引用照旧有效，
对照表在 [`packaging/README.md`](../README.md) 的索引里）。内容一字未改、只搬家：本行以上是 M145 新增的导航说明，
以下逐行取自原文件。

## 4. 每平台运行时依赖

### Windows x64（Win10/11）

| 依赖 | 来源 | 说明 |
|------|------|------|
| `KERNEL32` / `USER32` / `GDI32` / `ADVAPI32` / `SHELL32` / `ole32` / `WS2_32` … | 系统 | Win10+ 一律自带 |
| `ucrtbase` / `api-ms-win-crt-*`（UCRT） | 系统 | Win10+ 内置，**不需要** VC++ 可再发行包 |
| MSVC 运行时（`VCRUNTIME140` / `MSVCP140`） | **无** | `+crt-static` 静态链接，故不需要可再发行包 |
| OpenGL（`opengl32`） | 系统 | 只在 GL 渲染路径用；缺失走软渲染 |

本机（GNU 工具链）实测导入表证据见 §7。

### Windows ARM64（Win11 on ARM）

**本版不发布**（CI 照构建取证，红了不挡 x64 发布；等它绿了把键加回 `PUBLISH_SLOTS` 即可）。产物形态与门禁跟 x64
一样（裸 exe + 静态 CRT + dumpbin 导入表硬门禁），差别只在**怎么编**：**M140 起在原生 `windows-11-arm` 上编**
（GitHub 文档 2026 的 Standard runners 表明列该标签：4C/16G、公开仓库免费），产物落在 `target/release`。原路线
是在 `windows-latest` 上交叉到 `aarch64-pc-windows-msvc`，但**CI 上从未成功过一次**（v0.2.1 首次真跑即红在
「ARM64 工具链体检」）。M142 已确认那几轮「只剩一行 exit code 1」是**诊断脚本自己先死**（`set -uo pipefail` 关不掉
runner 注入的 `-e`，`find` 扫不存在的根被 pipefail 判非零就退出），已修；ARM64 的静态库在这台 runner 上到底缺不缺
要看下一次运行的新注解（固定 4 行实况清单，见 §3）。原生的附赠好处：宿主=产物架构，`ull --version` 冒烟**真跑**。

运行时依赖与 x64 同表（UCRT 是 Win11 ARM 的系统组件，`+crt-static` 静态链 CRT，不需要可再发行包）。
游戏侧如实说：**启动器原生运行**；Temurin 没有 windows-aarch64 产物，Microsoft OpenJDK 有该
发行版（core 的 `microsoft` 源按 `<os>-<arch>` 拼名，命名已覆盖），但 Minecraft 本体没有
Windows ARM64 原生库（Mojang/LWJGL 只发 x64/x86），实际游玩仍走系统 x64 模拟层 + x64 JDK。
所以本槽位只承诺「启动器产物」支持，不替 Java/游戏管理做架构级承诺。

### Linux x64

| 依赖 | 说明 |
|------|------|
| glibc ≥ 2.35 | 由 `ubuntu-22.04` 构建决定（该槽位默认不编，`all_slots=true` 取证跑才有产物）；更老的发行版请自行重建或等 musl 版 |
| `libxkbcommon.so.0` | winit 键盘处理 |
| `libfontconfig.so.1` + `libfreetype.so.6` | 文本渲染 |
| `libGL.so.1` / `libEGL.so.1` | Slint 版：femtovg 的 GL 渲染；iced 版：wgpu 的 GL/EGL 后端（**推断，未实测**）。两者缺失都退到 CPU 软渲染（tiny-skia） |
| `libxcb.so.1` / `libX11.so.6` | X11 后端（Wayland 会话下也需 XWayland 相关库） |
| `libwayland-client.so.0` | Wayland 后端 |

Debian/Ubuntu 一行：`sudo apt install libxkbcommon0 fontconfig libgl1 libegl1 libxcb1 libx11-6 libwayland-client0`。
运行期**不需要** `-dev` 包、不需要 Java（Java 是启动游戏时才由启动器自己管理的东西）。

**M118（D26）口径**：除上表里标注「推断，未实测」的那一行外，这张表与下面这条结论都是 **Slint 版 GUI** 的实测记录（M82：`readelf -d` + `ldd` + `nm -D`）；iced 版（`crates/app-iced`）的 `DT_NEEDED` / `dlopen` 集合**尚未重测**，请按「同一族库、细节待验」读，别当成 iced 版的实测承诺。
仍然成立的部分：上表是**开窗**要用的库，不是进程启动门槛——实测（§7.4，Slint 版）除 `libfontconfig.so.1` 外它们**都不在** ELF 的 `DT_NEEDED` 里，由 `dlopen` 在开窗那一刻才加载，所以没有 X11 / Wayland / GL 的机器上 `unlucky-launcher instances list` 照样跑得起来，`ull` 连 fontconfig 都不需要。
arm64 侧同理（同一份依赖树；历史口径里的「winit + femtovg」中 femtovg 只属于 Slint 版）。

### Linux arm64（aarch64）

依赖与 x64 完全同一套（`libxkbcommon` / `fontconfig` / `libGL` / `libxcb` / `libwayland-client`），
**glibc 地板也相同**：arm64 槽位跑在原生 `ubuntu-22.04-arm` runner 上（默认矩阵不编它，只有 `all_slots=true` 取证跑才出产物），地板 = **2.35**，与 x64
用的 `ubuntu-22.04` 一致。选 22.04 而不是 24.04 就是为了这个地板——24.04 的 glibc 更高，会把
Debian 12（2.36）、Raspberry Pi OS Bookworm（2.36）这些实际在跑的 ARM 发行版挡在门外。
要抬地板（例如将来 ARM 生态整体上移）换 `release.yml` 里那个槽位的 `os` 即可，脚本与产物名都不用动；
`ubuntu-24.04-arm` / `ubuntu-26.04-arm` 都是同一家族的标准公开仓库标签（4C/16GB、免费不限量）。

### macOS 12+

| 依赖 | 说明 |
|------|------|
| 系统框架（Cocoa / Metal / IOKit / Security …） | 由操作系统提供，无第三方动态库 |
| `libSystem` | 系统 |

macOS 产物是**裸二进制**而非 `.app` bundle：符合单文件承诺，但没有 Dock 图标、Finder 双击不生效，
从终端启动（README 里给了 `xattr -d com.apple.quarantine` 的 Gatekeeper 解法）。
签名/公证（需 Apple Developer 账号）与 `.app` 打包是后续工作，不做假承诺。

## 5. 软渲染兜底（无 GPU / 老驱动 / 远程桌面）

**现役入口（iced 版）**：`unlucky-launcher-iced --software-renderer`（等价 `--renderer=cpu`，即 `ICED_BACKEND=tiny-skia`）
走 tiny-skia 光栅化，这是现役构建唯一的软渲染入口——CI 的 runtime-smoke 与 `packaging/linux-smoke-docker.sh` 都这么跑（§11）。

**历史资产（`v0.1.0` 的 Slint 版）**：那批已发布二进制仍认 `unlucky-launcher --software-renderer`
（解析在 `crates/app/src/main.rs` 的 `apply_renderer_flag()`，只认完全相等的那个串）。
这条 Slint 路径（默认 `renderer-femtovg`/GL）已不在默认构建与发布矩阵内——`crates/app` 被 workspace `exclude`，
源码仅作参考、不编译——所以它**对现役 `unlucky-launcher-iced` 无效**，别照抄到 iced 版上。

- 软渲染是 CPU 光栅化，无 GPU 也能出图；代价是动画与大窗口重绘更慢（Metro 纯色界面影响有限）。
- 截图通道 `--screenshot <png>`：历史实现 `crates/app/src/screenshot.rs`，现役 `crates/app-iced/src/shot/`；
  软渲染下照样出图，是 CI 与远程机唯一可靠的"真出图"证据（发布冒烟见 §7，本机实跑 `--software-renderer --screenshot`）。
