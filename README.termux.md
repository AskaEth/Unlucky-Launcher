# Unlucky Launcher · Termux 适配（`termux-glibc` 分支）

这条分支把启动器带到 **Termux（Android）**：用 glibc 工具链编出 `aarch64-unknown-linux-gnu`
二进制，在 **termux-x11** 里跑 GUI；同时补上 Termux 平台识别、系统 JDK 安装与中文字体。

> 给主线的改动清单、兼容性说明与合并指南见
> [`TERMUX-ADAPTATION.md`](TERMUX-ADAPTATION.md)。

## 已验证

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 源码构建（GUI + CLI） | ✅ | Rust 1.99.1 gnu + clang/lld + glibc 运行时；GPU 构建（wgpu）默认带 CPU 兜底 |
| 窗口显示 | ✅ | termux-x11 `:0`，横屏 1657×1067，首帧 ~60 ms（GPU） |
| 中文 | ✅ | 启动时注册 `/system/fonts/NotoSansCJK-Regular.ttc` |
| 微软登录 id | ✅ | 构建期注入，日志 `client_id=configured source=build` |
| 横屏适配 | ✅ | 默认窗口按 X 屏幕等比自适应（`source=termux-screen`） |
| Java 发现 | ✅ | `JAVA_HOME`/PATH + `$PREFIX/lib/jvm` 扫描 |
| Java 自动安装 | ✅ | `ull java install 17` 由内置镜像装完 openjdk-17（约 20 s） |
| GPU（wgpu） | ✅ | 自编 Mesa 26.2.6 + KGSL；wgpu 选中 Adreno 840（Vulkan/turnip） |
| Minecraft 运行 | ✅ | 真机 1.20.4 跑通：glibc JRE + arm64 natives + softpipe GL，主菜单在 X11 渲染 |

## 快速开始

完整配方（工具链、patchelf、linker、门禁）见
[`packaging/docs/termux-glibc-build.md`](packaging/docs/termux-glibc-build.md)。最短路径：

```sh
termux-x11 :0 -ac &
DISPLAY=:0 ./unlucky-launcher-iced        # 默认 auto：有驱动走 GPU，没有则自动降级 CPU
```

- GPU 驱动不随 Termux 源提供，需要按构建文档 §5 自己编一次 Mesa 26 + ICD；
  不装也能跑，只是 `auto` 会落到 tiny-skia（CPU）。
- 想强制某一侧：`--renderer=gpu` / `--software-renderer`。
- 窗口尺寸默认按屏幕自适应；`ULL_WINDOW_SIZE=1520x900` 可覆盖。
- Java：`ull java install 17|21|25` 在 Termux 上默认用系统包（内置镜像 + 隔离 apt 状态）。
- **数据根**：Portable 模式按**可执行文件所在目录**生成 `.ULL` / `.minecraft`；
  换目录启动会看到另一套实例（`--data-dir` 目前接受并忽略）。

## 已知边界

- 仅单机（OnePlus 15 / Android 17）验证，试验级、不在发布矩阵内。
- **GPU**：已打通，但驱动是自编的（Mesa 26.2.6 + KGSL），装到
  `$PREFIX/glibc/{lib,share/vulkan/icd.d}` 后应用会自动启用；Termux 官方源里的
  glibc Mesa 太旧（24.2.6 不认识 Adreno 840）。驱动缺失时自动降级 CPU，不会崩。
- **MC**：Android 原生 natives 走不通（LWJGL 没有官方 Android/bionic 产物，且 GLFW 的
  窗口模型与 termux-x11 的 X11 不兼容）。有效路线是 glibc JVM，已真机跑通 1.20.4：
  Temurin 21 JRE（patchelf 后直接执行）+ Linux arm64 natives（启动器自动改写/镜像兜底）
  + softpipe GL。非致命项：narrator 的 flite 缺失、LWJGL JNI 版本告警；
  softpipe 是软件渲染，帧率有限，zink（硬件 GL）仍卡 DRI3。
- Termux 上三条测试失败是环境差异（`/proc/*/children`、长路径脱敏、mtime flaky），
  PC 的 Linux CI 不受影响。

## 后续

1. zink 的 DRI3 问题（硬件 GL；当前 softpipe 可跑但慢）——这是 MC 帧率的主要瓶颈
2. 剪贴板/IME 在 termux-x11 下的行为验证
3. narrator/flite 与音频设备（当前 missing 但不阻断）
