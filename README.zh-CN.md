# Unlucky Launcher (ULL)

[English](README.md) · **简体中文**

> **官方项目主页**：https://github.com/AskaEth/Unlucky-Launcher

Rust 原生单文件 Minecraft 启动器：**Slint + Metro 视觉**，内置 **AI Agent 客户端**（对话 / 改配置 / 崩溃诊断），
融合主流 Agent 工具（**MCP**），**最低基线 Win10+**。

- 运行时：纯 Rust 原生二进制——无 JVM、无 WebView、无外部运行时
- UI：Slint（Royalty-Free 2.0；GL 优先，CPU 软渲染兜底，无可用 GPU 的老机也有完整 UI）
- 许可：Apache-2.0 © AskaEth (YeiJ)

## 当前状态

**核心引擎已完成**（workspace 983+ 测试全绿）：

- **正版登录**：Microsoft OAuth 2.0 device code flow → Xbox Live → XSTS → Minecraft Services，校验游戏拥有状态
- **账户凭据仅保存在本机**，不上传任何服务器，AI 功能无法访问
- 加载器：Forge / NeoForge / Fabric / Quilt 四家真机验证
- Java 管理：Temurin / Oracle / Microsoft / Zulu 四发行版回退链
- 多源下载引擎、崩溃诊断规则引擎
- 可作为 MCP server 接入外部 Agent 工具（`ull agent serve`）
- 本地离线档案：仅用于本地/隔离网络环境的测试，不经过 Microsoft 账户认证，无法连接启用正版验证的服务器

**CLI（`ull`）完整可用**：`install / launch / accounts / java / diagnose / agent` 等全量命令。

**图形界面（GUI）开发中**：`crates/app` 当前为 Slint + Metro 风格骨架，主题系统与完整界面
随后续版本交付——在此之前请使用 CLI。

## 模块地图

```
Unlucky-Launcher (cargo workspace)
├── crates/core     # 领域库：零 UI 依赖，八模块
├── crates/agent    # AI Agent：ToolSpec 注册表 + 内置客户端 + MCP server（双形态共用一个内核）
├── crates/app      # Slint UI：唯一允许依赖 UI 框架的 crate
├── crates/cli      # CLI（bin `ull`）：headless，零 UI，只依赖 core + agent
└── packaging       # 单文件产物矩阵与打包资源（规划中）
```

`crates/core` 的八个模块：

| 模块 | 职责 |
|------|------|
| `setting` | 三模式数据目录、schema 配置、原子写、迁移 |
| `download` | 多源健康度、镜像、分块下载、边下边算 |
| `game` | 版本/实例仓库、快照与继承解析 |
| `launch` | 10 阶段启动管线、进程、JSONL 事件流 |
| `auth` | Microsoft 正版登录 / 本地离线档案 |
| `addon` | 加载器安装、整合包、CurseForge / Modrinth |
| `diagnose` | 五类信号采集、YAML 规则引擎、hs_err 解析 |
| `platform` | OS 抽象：路径、进程、文件锁、能力探测、运行期 OS 检测 |

## 构建

前置：

- Rust stable（`rust-toolchain.toml` 会自动选择工具链；组件 `rustfmt` / `clippy`）
- Windows：MSVC 工具链（CI 用）或 MinGW-w64（`x86_64-pc-windows-gnu`）
- Linux：`libxkbcommon-dev libfontconfig1-dev libgl1-mesa-dev libegl1-mesa-dev libwayland-dev libxcb1-dev`
  （Slint winit 后端 + femtovg 编译依赖）

```bash
cargo fmt --all --check                              # 格式
cargo clippy --workspace --all-targets -- -D warnings # 静态检查（零 warning 门禁）
cargo test --workspace                               # 测试
cargo build --release                                # 产物：target/release/unlucky-launcher[.exe]
```

软渲染开关（无可用 GPU / 烂驱动的老机）：`unlucky-launcher --software-renderer`
（等价于 `SLINT_BACKEND=winit-software`）。

依赖许可检查（本地可选，CI 必跑）：`cargo deny check licenses bans sources`。

## CI 门禁

| Workflow | 内容 |
|----------|------|
| `.github/workflows/ci.yml` | fmt + clippy + test + build，Windows / Linux 矩阵 |
| `.github/workflows/guards.yml` | core 零 UI 守门、单文件 ≤400 行守门、cargo-deny 许可守门 |

## 平台基线

最低基线 **Windows 10**。Windows 8.1 / 8 / 7 不在支持范围内：

- Windows 10/11 走**标准工具链**产物（MSVC / GNU），无自定义 target spec、无 `-Zbuild-std`、无导入表白名单
- 其他平台：macOS 12+ 一等；Linux x64 X11 优先、Wayland 为**大方向**；Android Termux 试验级不承诺
- UI 兜底：Slint 软渲染（无 GPU / 烂驱动的老机可完整使用）
- 平台独立性体现在**独立测试矩阵**，而非独立产物链；能力差异用 feature gate + 运行期探测表达

## 许可证与致谢

- 本项目：Apache License 2.0，见 [LICENSE](LICENSE)；第三方声明见 [NOTICE](NOTICE)
- GUI 使用 [Slint](https://slint.dev)（Royalty-Free 2.0 轨道）。按 RF 2.0 义务，关于页将放置
  `AboutSlint` 部件（P2），在此之前由 NOTICE 与本节承担告知义务

感谢 [HMCL](https://github.com/HMCL-dev/HMCL) 和 [PCL2](https://github.com/Meloong-Git/PCL)（均 GPL-3.0）这两个启动器陪伴了绝大多数 MC 玩家好多年。
ULL 的设计与实现从它们身上学到了太多，谨以此项目向两位前辈致敬。
