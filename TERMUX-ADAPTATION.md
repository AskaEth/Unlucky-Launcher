# Termux 适配：方案、改动清单与合并指南

> 分支：`termux-glibc`（基于 `origin/dev`，15 个提交，分叉点 `87ab878`，合并前 HEAD `e987e02`）
> 目的：让主线快速了解这组代码改了什么、为什么、怎么验证、如何合并。

## TL;DR

- **构建**：为 Termux 编出 `aarch64-unknown-linux-gnu`（glibc）二进制，在 termux-x11 里跑 GUI。
  Android 原生 target 走不通（winit 的 android 后端需要 NativeActivity），所以不用它。
- **已跑通**：GUI（CPU 与 GPU 两条渲染路径）、中文、横屏自适应、JDK 发现与系统包安装、
  MC 1.20.4（glibc JRE + arm64 natives + softpipe GL，真机进游戏）。
- **未收口**：MC 启动参数支持（由主线后续补）、MC 硬件 GL（zink 卡 DRI3）、剪贴板/IME。
- **对主线的影响**：所有行为都由 **运行时 Termux 识别 / `cfg(target_os = "linux")` /
  文件存在** 三选一 gate 住，Windows / macOS / 普通 Linux 的默认路径不变。

## 方案

### 1. 平台识别

`crates/core/src/platform/termux.rs`：用 `TERMUX_VERSION` 或默认 `PREFIX`
（`/data/data/com.termux/files/usr`）识别 Termux，推导 `$PREFIX` 与 `$PREFIX/lib/jvm`。
不把路径写死到调用方，也不改 `OsFamily`（Android 上仍按 Linux 家族处理）。

### 2. JDK：发现 + 安装

- **发现**：`game/java/discover.rs` 的 Unix 扫描根追加 `$PREFIX/lib/jvm`，
  环境可注入（`standard_roots_in`）。补上「Termux 装了 openjdk 但没设 JAVA_HOME/PATH」的盲点。
- **安装**：新增 `game/java/termux.rs`。Termux 的 openjdk 是 `.deb`（bionic），
  与 Temurin 的 zip/tar.gz 不是同一形态，所以**委托包管理器**：
  - 内置 SAU / TUNA / USTC / 官方四个 termux-main 镜像，按顺序 `apt-get update && install`；
  - lists/cache 隔离在 `$PREFIX/tmp/ull-java-termux/`，不污染用户 apt 配置；装完清理；
  - 装完从 `$PREFIX/lib/jvm` 定位，幂等（已装则不走网络）。
- **vendor 接线**：`JdkVendor::Termux`（17/21/25）。Termux 上缺省安装先试系统包，
  失败再走归档链；`--vendor termux` 只走系统包。设置白名单与契约测试同步。

### 3. 字体

`crates/app-iced/src/fonts.rs`：Termux 上把 `/system/fonts/NotoSansCJK-Regular.ttc`
注册进 cosmic-text（fontdb 不扫 Android 字体目录），中文不再显示方框。

### 4. 窗口（横屏优先）

`app/boot/termux_window.rs`：Termux 上默认窗口按当前 X 屏幕等比自适应
（日志 `size_source=termux-screen`），横屏铺满、竖屏不溢出；
`ULL_WINDOW_SIZE` 覆盖与 1180×760 验收基准不变。

### 5. GPU（启动器）

- 驱动是自编的 **Mesa 26.2.6 + KGSL**，装在 `$PREFIX/glibc/{lib,share/vulkan/icd.d}`；
  构建/安装步骤见 `packaging/docs/termux-glibc-build.md` §5。
- `app/boot/termux_gpu.rs`：glibc 构建 + Termux + ICD 文件存在时，在探测前设
  `VK_ICD_FILENAMES` / `VK_DRIVER_FILES`；用户显式设置不覆盖。
- 实测 wgpu 选中 Adreno 840（turnip Mesa 26.2.6）；无驱动自动降级 tiny-skia。

### 6. Minecraft 运行链

- **natives**：1.19+ 的清单把 natives 作为独立库条目（`org.lwjgl:...:natives-linux`），
  Linux/aarch64 上改写成 `natives-linux-arm64`（`Library::linux_arm64_native_name`，
  install 与 launch 共用）；Mojang 的 `libraries.minecraft.net` 对 arm64 一律 404，
  下载兜底到阿里云 Maven（`install/libraries.rs`）。
- **JVM**：用 glibc 的 Temurin JRE（patchelf 后直接执行），bionic 的 Termux JVM
  加载不了 glibc natives。
- **GL**：glibc Mesa 的 zink 在 termux-x11 下卡 `DRI3 not available`，
  子进程默认带 `MESA_LOADER_DRIVER_OVERRIDE=swrast` + `LIBGL_ALWAYS_SOFTWARE=1`
  （`launch/defaults.rs`，用户自定义环境变量可覆盖）。

## 改动清单

| 文件 | 改动 | 平台隔离 |
| --- | --- | --- |
| `core/platform/termux.rs`（新） | Termux 识别、`$PREFIX`/JVM 根 | 纯函数，按环境判断 |
| `core/platform/mod.rs` | 导出新模块 | — |
| `core/game/java/discover.rs` | `$PREFIX/lib/jvm` 扫描 | Unix + Termux 判断 |
| `core/game/java/termux.rs`（新） | 镜像/隔离 apt 安装、`installed_home` | 仅 Termux 可执行 |
| `core/game/java/vendor.rs` / `source.rs` / `install.rs` | `JdkVendor::Termux` 与安装分派 | Termux 上才启用 |
| `core/setting/keys.rs` + 契约测试 | 白名单加 `termux` | 跨平台可选，非 Termux 明确报错 |
| `cli/commands/java_vendors.rs`、`cli/cli/help.rs`、`presenter/host/java.rs` | 展示与帮助文案 | 展示层 |
| `core/game/library.rs` | `linux_arm64_native_name` / 分类器改写 | 仅 linux+arm64 |
| `core/game/install/libraries.rs` | 现代 natives 条目改写 + Maven 兜底 | 仅 linux+arm64 |
| `core/launch/defaults.rs` | Termux softpipe 环境默认值 | `cfg(linux)` + Termux |
| `presenter/host/launch.rs`、`cli/commands/snapshot.rs` | classpath 用改写后的 arm64 坐标 | 仅 linux+arm64 |
| `app-iced/fonts.rs` | Android CJK 字体注册 | 仅 Termux |
| `app-iced/boot/termux_window.rs`（新） | 横屏自适应 | 仅 Termux |
| `app-iced/boot/termux_gpu.rs`（新） | 自动启用自编 ICD | 仅 glibc+Termux+文件存在 |
| `app-iced/Cargo.toml` | 新增 `unlucky-core` 依赖 | 复用平台识别 |
| `README.termux.md`（新）、`packaging/docs/termux-glibc-build.md`（新） | 分支说明与构建配方 | 文档 |
| `README.md` / `README.zh-CN.md` / 支持矩阵两处 | Termux 行同步 | 文档 |

## 行为变化与兼容性

- 新增 `JdkVendor::Termux` 会改变 `VENDOR_IDS` / `JDK_VENDORS` 的允许值集合
  （契约测试已同步）。非 Termux 上显式选它会在安装时明确报「只在 Termux 里可用」，
  默认回退序**不含** termux，PC 行为不变。
- Linux/aarch64 的 LWJGL3 natives 会被改写；x86_64、Windows、macOS 不受影响。
- `app-iced` 直接依赖 `unlucky-core`（此前通过 cli/presenter 间接依赖）。
- 所有 Termux 专属路径都需要运行时识别或文件存在才会生效；普通 Linux 桌面不会触发。

## 合并指南

1. ~~`git fetch origin && git rebase origin/dev`~~ → **实际按用户令走 `--no-ff` 合并**
   （M156）：`git merge --no-ff origin/termux-glibc`，15 个原提交一字未改地留在历史里，
   按平台识别 → 字体 → 文档 → Java → GPU → natives → MC 的分组可整组 revert。
   与 dev 前进的 12 个提交**零文本冲突**（交集只有 `Cargo.lock` / 两个 README /
   `crates/app-iced/Cargo.toml`）。
2. PR base：`dev`。CI 门禁：`fmt` / `clippy --workspace --all-targets -D warnings` /
   `test --workspace --all-targets` / `cargo deny`（本机没装 deny，PC 上补跑）。
3. 本机已知、**不影响 PC CI** 的三条失败：
   - `unlucky-core --test launch`：`/proc/<pid>/task/<pid>/children`（Android 内核没有）
   - 同测试：长路径被脱敏成 `<opaque>`
   - `presenter settings_bridge`：mtime 热重载偶发（flaky，单跑 3 次 2 过 1 挂）
4. **不要提交**：`dist/`、`.gnu/`、任何 `ULL_MSA_CLIENT_ID` 值（仓库 Secret）。
5. 合并后建议真机再跑一次：`--software-renderer` 截图 + `ull java install 17` + MC 启动。
6. **PC 门禁发现的两处、已在 M156 就地修掉**（都只在 Windows/PC 侧才暴露，
   所以在 Termux 上跑门禁看不出来）：
   - `game/java/discover.rs` 的两个 `#[cfg(unix)]` 测试，导入没跟着 gate
     → Windows 上 `-D warnings` 判「未使用导入」。已把两个 `use` 同样 gate 成 `#[cfg(unix)]`；
     代价是这两个用例在 Windows 上不编译（由 Linux CI 覆盖）。
   - 本分支把 `game/java/install.rs`（456 行）与 `game/install/libraries.rs`（409 行）
     顶过了 400 行红线（`guards.yml` 的 `file-size`）。已按步骤拆出
     `install/termux_install.rs`、`install/one.rs`、`install/libraries/arm64_natives.rs`，
     行为不变、用例原样留在原处。

## 验证矩阵（本机）

| 项 | 结果 |
| --- | --- |
| GUI 截图（横屏 1657×1067） | ✅ 中文正常，`client_id=configured source=build` |
| GPU 渲染 | ✅ wgpu 选中 Adreno 840（turnip Mesa 26.2.6）；降级路径也验证过 |
| `ull java install 17` | ✅ SAU 镜像约 20 s，装完清理，`java list` 可见 |
| MC 1.20.4 | ✅ 真机安装（740 MB）→ 主菜单 → 建世界 → 进游戏 |
| fmt / clippy / 核心测试 | ✅ 475 条核心测试通过，clippy 零告警 |

## 后续路线

1. **MC 启动参数**（主线功能，另行排期）：当前 `ull launch` 不接受自定义 JVM/游戏参数。
2. **MC 硬件 GL**：修 glibc GLX 的 DRI3（让 zink 生效），或改用 VulkanMod。
3. 剪贴板 / IME 在 termux-x11 下的行为验证。
4. narrator/flite 与音频设备（当前缺失但不阻断）。
5. GPU 驱动分发：目前每次都要按文档自编 Mesa；可考虑脚本化/预编译产物。

## 回滚策略

全部改动按提交分组（platform → fonts → docs → java → gpu → natives → mc），
可按组 revert；Termux 专属逻辑都有运行时/文件存在 gate，回滚不会波及 PC 路径。
