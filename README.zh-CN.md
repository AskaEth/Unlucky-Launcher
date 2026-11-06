# Unlucky Launcher (ULL)

[English](README.md) · **简体中文**

> **官方项目主页**：<https://github.com/AskaEth/Unlucky-Launcher> ｜ 下载、议题、发布说明都在这里：
> <https://github.com/AskaEth/Unlucky-Launcher/releases>

Unlucky Launcher 是一个用 Rust 写的免费开源 **Minecraft: Java 版**启动器（Apache-2.0）。每个平台就是
**一个原生二进制**——没有安装包、没有捆绑运行时、没有 WebView——从装一个版本到读懂一次崩溃，它把启动器
该做的事都覆盖了：实例与版本、加载器与整合包、Mod / 资源包 / 光影、逐实例 Java、微软 / 第三方社区服 /
本地离线账户，以及一套在本地按 YAML 规则跑的崩溃诊断。你的数据不经过我们自己的任何服务：账户令牌只
留在本机。

![实例页：左侧是本地实例列表，右侧是所选实例的版本、加载器、隔离方式与上次游玩时间](docs/images/instances-en-light.png)

*实例页——1180×760，英文界面，浅色档。实例名是测试数据，不是谁的真实存档。图里那句
`Launch failed · exit0 静默失败` 也正好是一处已知缺口的诚实样子：那行原因仍是启动层直接抛出来的中文
原文（见[还没做](#还没做)）。*

## 现在能用什么

### 实例、版本与启动

- **实例**——list-detail 版式：左侧是本地实例列表；右侧是所选实例的版本、加载器、隔离方式（独立游戏
  目录或共享目录）、文件是否在盘上、上次游玩时间，以及 Play / Stop。窄窗是把两个面板整页切换，不是上下堆。
- **实例从哪来**——装一个版本（带实例名、加载器与加载器版本），或者导入一个整合包。实例页自己的
  **新建实例 / 导入 / 复制 / 打开文件夹 / 删除** 五个按钮**还没接后端**：它们是占位动作，点了只往
  stderr 写一行（`crates/app-iced/src/app/update/page_messages.rs`），欠的是实例向导
  （`crates/app-iced/src/pages/mod.rs` 偏离 #8）。
- **版本管理**——三栏（版本类型 / 主版本 / 搜索）读 Mojang 版本清单：正式版、快照、远古版；安装时可选
  加载器与加载器版本。取消安装是真停：在下一个段边界收手，在飞的那个文件就地掐断，且不登记实例
  （`crates/presenter/src/host/dispatch.rs:61-88`）。命令行还能只出计划不下载（`--dry-run`：按类的文件数
  与总大小，人类可读输出里每类给一个源链样本；整份文件清单在 `--json` 里）。
- **启动**——10 阶段命令行组装（`crates/core/src/launch/`）、逐实例 JVM 与游戏参数、原生库解压落位，
  每次运行留一份 JSONL 会话记录。进程按**进程树**杀，不是只杀一个 PID。
- **崩溃诊断是真的、也是本地的**——`crates/core/src/diagnose` 采五类信号（退出码、`latest.log`、
  `crash-reports`、`hs_err_pid`、含 mod 清单的环境快照），按 YAML 规则集匹配，把结论与修复动作显示在
  日志页上。不接模型、不联网：断网能用，没有 GPU 的机器也能用。

### 加载器、整合包与内容库

- **加载器**——Fabric、Quilt、NeoForge、Forge。需要跑官方 installer `processors` 的 Forge/NeoForge 版本
  会真跑一遍；机器上没有 Java 时先装一个托管 JDK。
- **整合包**——导入 CurseForge 清单、Modrinth `.mrpack`、MultiMC/Prism 实例；导出为 `.mrpack` 或
  CurseForge 清单。**导入**的取消只是界面级：面板立刻收回、结果作废，但后台那次导入会跑到底——core 的
  导入请求还没有取消令牌（`crates/presenter/src/host/addon/pack.rs:28-42`）。
- **内容库**——整合包 / Mod / 资源包 / 光影，带关键词、来源、版本、加载器与排序筛选，分页结果，
  以及一键**装入这个实例**。
- **这一页会拒绝什么，就写在那一行上**——整合包**不能**从内容库直接装（整合包走上面的导入管线）；
  CurseForge 的「装入」是禁用态，因为它的文件端点还没接通；游戏版本或加载器对不上目标实例的条目也是
  禁用态，并把不匹配写清楚。没有一条是静默失败的。
- **CurseForge 检索要 key**——启动前设 `ULL_CF_API_KEY`，没有它这个来源在来源筛选里是灰的。key 只活在
  环境变量里：不写进配置文件、不进日志（`crates/presenter/src/host/addon.rs`）。

### Java

逐实例的托管 JDK，四家发行版的回退链**按地区排**——Temurin 打头，国内是 Oracle 先于 Microsoft
（那边实测快 28×）、国外反过来，Zulu 收尾——可用 `java.vendorPriority` 设置整体覆盖
（`crates/core/src/game/java/vendor.rs`）。镜像也按地区选。`java list` 把它真能找到的每个 JDK 都列出来
（包括官方启动器自己下的那些），`java install <主版本>` 拉一个。游戏要的 JDK 是启动器的数据，不是启动器
本身的依赖。

### 账户

三条轨道在同一页：

- **微软**——OAuth 2.0 设备码流程 → Xbox Live → XSTS → `api.minecraftservices.com`。
- **第三方社区认证服**（authlib-injector 一类）。
- **本地离线档案**，用于隔离测试。

**微软登录需要一个 Azure 公共客户端 id，而发布版二进制里没有。** 这个 id 不在仓库里，CI 也不注入，所以
启动前设 `ULL_MSA_CLIENT_ID=<id>`（或构建时注入：`ULL_MSA_CLIENT_ID=<id> cargo build`，它通过
`option_env!` 烧进二进制）。没有它时点登录**不会联网**，弹窗直接说清是这一项没配：标题「微软登录未配置」，
正文点名 `ULL_MSA_CLIENT_ID=<id>`（设好或构建时注入后要**重启启动器**——已在运行的进程读不到新设的环境
变量），并带 `代码：msa_not_configured`；它**不给重试按钮**，因为在同一个进程里重试必然是同一个错。其它
登录失败仍是老样子：一句分类文案 + `代码：<码>`，完整原因只在启动器日志里。第三方社区服与本地档案不需要 id。

### 外观与语言

- **只有一套 UI 栈**——iced 0.14 + Material 3 Expressive，自绘标题栏、带动效的导航轨，浅色 / 深色 /
  跟随系统。
- **外观**——窗口背景图与取色链路（面板配色可从壁纸推出来）、面板透明度、整窗透明度（Windows）。Windows 11
  与 Server 2025 上向系统请求外框圆角；macOS 与 Linux 交给桌面合成器自己的规则；我们不自己画圆角。
- **CPU 兜底**——`--software-renderer` 让没有可用 GPU 或驱动有问题的机器照样正常显示；没有可用适配器时，
  后端探测自己退到 CPU。这条路上**面板级**半透明与模糊会关掉（**窗口级**透明度照常可用），设置面板会写明。
- **本地化**——跟随系统语言探测、设置里可显式切换、重启后保持。绝大多数文案来自同一份消息目录
  （`crates/i18n`）。**边界，因为上面这句很容易被读大**：防硬编码的守门
  （`crates/app-iced/src/pages/instances/tests/i18n.rs`）只扫实例页源码，不是每一屏；写死的中文字面量里
  没有 ASCII 单词可以匹配，这一类它抓不到；外观面板的老控件还是硬编码英文
  （`crates/app-iced/src/pages/appearance/section.rs`），中文界面下它们仍显示英文。

### 助手，以及 MCP server

这是两件不同的事，README 不该含糊：

- **启动器内的助手今天还是占位壳。** 它的 transport 是脚本化的
  （`crates/app-iced/src/presenter/transport.rs`），没有接真实模型，而且界面上它出现的每一屏都用四处明说
  这一点——副标题、输入框占位符、空态正文，以及一枚「模拟」胶囊。会话状态、未读数、草稿、确认流程与具名
  失败码都是真的 presenter 状态；缺的只是它们后面的模型。
- **MCP server 是真的。** `ull agent serve` 把启动器内核以 MCP server 形态（stdio 或 Streamable HTTP）
  开放给外部 Agent 工具，只监听回环（`127.0.0.1`，默认端口 47823），除非
  `agent.mcp.allow_write=true` 否则只读。十个工具规格（`crates/agent/src/tools/`）同时喂给两种形态，两条
  路都过同一个执行器——schema 校验、敏感分级、审计留痕。`ull agent config` 能打印现成的 Claude Code /
  Cursor 配置片段。

### 命令行

GUI 二进制在跑任何 UI 代码之前就先认全部 CLI 子命令——不开窗、不探渲染器、不扫字体。另有一份更轻的 `ull`
产物，命令集相同，却一个图形栈都不链接。

```bash
unlucky-launcher                            # GUI
unlucky-launcher --software-renderer        # GUI，CPU 渲染
unlucky-launcher instances list             # CLI，不开窗
unlucky-launcher --json launch 1.20.1       # CLI，全局选项排在子命令前面
unlucky-launcher --version                  # CLI，版本 + 系统摘要
```

判据刻意收得很窄，所以原本能用的用法一条都没变：

- 第一个参数是裸词（不以 `-` 开头）→ 它就在子命令的位置上，交给 CLI。GUI 的 flag 永远不会出现在那个位置：
  `--screenshot` / `--diff` / `--data-dir` 的取值都排在 flag 之后。
- 第一个参数是 flag → 只有当 `crates/cli` 的解析器也解析出一条真命令时才算 CLI 调用（`--help`、
  `--version`、`--json instances list`）。单独 `--data-dir X` 解析成「没有子命令」，仍然开 GUI；GUI 专属
  参数（`--software-renderer`、`--screenshot*`、`--renderer`、`--theme`、`--diff`、`--force-state` 等）
  在 CLI 里一律解析失败，也仍然开 GUI。

**未知子命令是错误，不是开窗**：`unlucky-launcher lst` 打印「未知命令 `lst`」、提示
`unlucky-launcher --help`，以 1 退出。静默开窗会让脚本以为自己那个拼错的参数生效了。子命令表只有一处定义
（`crates/cli/src/cli/parse.rs`），所以两个二进制认的命令集天然一致。`ull --help` 会全列出来：`instances`、
`install`、`launch`、`pack import`、`accounts`、`sessions`、`logs`、`diag`、`config`、`java`、`agent`、
`version`、`help`。

### 还没做

这份清单是让人相信上面那些话的理由。没有一条藏在宣传话术后面。

- **没有安装包，也没有自动更新通道。** 下个文件直接跑，没有任何东西替你检查新版本，更新源也还没定。
- **有一批按钮是占位。** 实例页：*新建实例*、*导入*、*复制*、*打开文件夹*、*删除*。日志页：*打开文件夹*。
  设置页框架：*保存*、*重置本组*、*添加覆盖*。点下去只往 stderr 写一行 `[page] 占位动作 …`，什么都不改
  ——图形界面里的实例管理就是还欠着的那块实例向导；设置项是边改边生效，而不是靠「保存」。
- **Linux 与 macOS 产物未验证。** 参考机是 Windows；`linux-x64`、`linux-arm64`、`macos-arm64`、
  `macos-x64` 四份归档由 CI 矩阵产出，还没被下载下来端到端跑过。把它们当**未经测试的产出**，而不是
  「矩阵里有就算支持」。
- **macOS 未定型。** 有两轮真机证据（Apple Silicon），构建因此存在——但菜单栏显示名、三个窗口按钮的位置
  （macOS 习惯在左上，我们自绘在右上）、300s+ 稳态仍需真机复核。macOS 产物未签名，首次启动要自己清掉
  Gatekeeper 隔离标记（`xattr -d com.apple.quarantine`）。macOS 运行期冒烟是手动 `workflow_dispatch`
  的探针、带 `continue-on-error`，不是门禁。
- **内容库页在低窗口高度下会挤**，而且窗口高度还没接进页面模型——这一页按一个名义高度排版，而不是按它
  真正拿到的高度。
- **启动侧失败提示未结构化。** 磁贴直接采用启动层抛出的中文句子
  （`crates/core/src/launch/exit_analysis.rs`），而不是一个界面能翻译的原因码——上面那张英文截图里的
  `静默失败` 就是它。
- **启动器内的助手背后没有模型**（见上）。MCP server 有。
- **整合包导入只有两份全局策略**：保留用户侧或覆盖，不是逐文件选择；导入报告会逐条列出冲突，所以没有
  文件是被静默覆盖的。
- **内容库的安装进度是阶段级**，不是字节级。
- **没有原生文件选择框。** 导入靠拖拽或粘贴路径。
- **Android/Termux 是试验级、不承诺**：发布矩阵里没有面向它的资产，那一侧的 GUI 也未验证。

## 下载与运行

从 [Releases](https://github.com/AskaEth/Unlucky-Launcher/releases) 拿你那个平台的文件。文件名形如
`unlucky-launcher_<版本>_<平台>_<架构>`；这个形状的唯一真相源是
`packaging/make_artifacts.py::asset_stem()`：

| 槽位 | 产物 | 形态 |
|------|------|------|
| `windows-x64` | `unlucky-launcher_<版本>_windows_x64.exe` | 裸 `.exe`，双击即用 |
| `windows-arm64` | `unlucky-launcher_<版本>_windows_arm64.exe` | 裸 `.exe` —— **暂未发布**，见下 |
| `linux-x64` | `unlucky-launcher_<版本>_linux_x64.tar.gz` | 单个归档，解压出一个顶层目录 |
| `linux-arm64` | `unlucky-launcher_<版本>_linux_arm64.tar.gz` | 同上 |
| `macos-arm64` | `unlucky-launcher_<版本>_macos_arm64.tar.gz` | 同上 |
| `macos-x64` | `unlucky-launcher_<版本>_macos_x64.tar.gz` | 同上 |
| 每个槽位 | `ull_<版本>_<平台>_<架构>[.exe]` | 裸 CLI 文件，独立资产 |
| 全平台 | `SHA256SUMS.txt` | `sha256sum -c` 可直吃 |

`windows-arm64` 是表里唯一「写了但**本次未发布**」的一行：CI 的 ARM64 runner 上缺 Windows SDK 的
ARM64 导入库——原因已定位，正在修。这一行不删，是因为架构仍然要支持、槽位也仍在矩阵里构建（只是不进
Release）；决定一个槽位能否进 Release 的是 `.github/workflows/release.yml` 里的 `PUBLISH_SLOTS`，
它现在只写了 `windows-x64`——所以 `windows-x64` 是当前唯一可下载的 Windows 资产。

UNIX 归档里是二进制加 `LICENSE`、`NOTICE`、`README.md`，都在一个顶层目录下。Windows 发的是裸 `.exe`
（一个文件装不下这三份文本），所以这三份作为并列资产挂在 Release 上——下 Windows 二进制不会把许可条款
弄丢。下载链接里不带实现细节字样，归档里的二进制名永远是 `unlucky-launcher`。

这两种形态不是重复品。GUI 二进制开窗时要用图形栈（X11 / Wayland / GL 走 `dlopen`，启动时直接链接
`libfontconfig`）；`ull` 除了 glibc 什么都不需要，所以在没有窗口的服务器、CI 容器上跑得起来。Linux 上这个
地板是 glibc ≥ 2.35（槽位在 Ubuntu 22.04 上构建）——更老的发行版得源码自建（`packaging/README.md` §4）。
没有桌面就用 `ull`，有桌面就用 `unlucky-launcher <子命令>`。两者打出来的 CLI 文案是同一套，自称的名字跟着
**你敲的那个文件**走——`argv[0]` 去掉目录与结尾的 `.exe`，别的都不剥
（`crates/cli/src/cli/program.rs`），所以 `ull` 打印 `ull ·`，归档里那个二进制打印
`unlucky-launcher ·`，本地 `cargo build` 出来的打印 `unlucky-launcher-iced ·`。

最低平台基线：**Windows 10**；Windows 8.1 / 8 / 7 不在支持范围内。macOS 12+（Apple Silicon 与 Intel），
Linux x64 / arm64（基线判定在 `crates/core/src/platform/os_info.rs`）。Windows 10/11 的产物由标准工具链
（MSVC / GNU）构建，构建方式与上游 cargo 一致。Linux 上 X11 是当前首选路径，Wayland 是大方向。能力差异
一律用 feature gate + 运行期探测表达，绝不做成「这个平台天生少一块」的编译期裁剪。

第一次用微软账户？先设 `ULL_MSA_CLIENT_ID`——见[账户](#账户)。其它都不需要配置。

## 从源码构建

前置：

- Rust stable（`rust-toolchain.toml` 会自动选工具链；门禁还需要 `rustfmt` / `clippy` 组件）
- Windows：MSVC 工具链（CI 用这套）或 MinGW-w64（`x86_64-pc-windows-gnu`）
- Linux：`libxkbcommon-dev libfontconfig1-dev libgl1-mesa-dev libegl1-mesa-dev libwayland-dev
  libxcb1-dev`——winit + wgpu/tiny-skia 编译期要用

```bash
cargo fmt --all --check                                # 格式
cargo clippy --workspace --all-targets -- -D warnings # 静态检查（零 warning 门禁）
cargo test --workspace                                 # 测试
cargo build --release                                  # bin 落在 target/release/
cargo deny check licenses bans sources                 # 依赖许可（CI 必跑）
```

编出来的 bin：

- `unlucky-launcher-iced`——GUI，同时也是 CLI：同一个文件认上面列的全部子命令。名字里的 `iced` 是 cargo
  的 bin 名，不是下载名——`packaging/make_artifacts.py` 把同一次构建落成
  `unlucky-launcher_<版本>_<平台>_<架构>`，归档里的二进制名写成 `unlucky-launcher`。
- `ull`——CLI 的独立无 UI 构建：同一套命令、不链接图形栈、体积更小，给完全没有显示设备的机器用。

渲染开关：

- `unlucky-launcher-iced --software-renderer` 强制 tiny-skia（`ICED_BACKEND=tiny-skia`）；
  `--screenshot <png>` 出一帧图后退出——这是 CI 无头冒烟 job 用的验收通道
- 没有可用适配器时，iced 版靠显式后端探测自己退到 CPU

**CI 门禁。** 每次推送到 `main` 与每个 PR 都跑 fmt / clippy（warnings 视为错误）/ 全量测试，Linux 无头
渲染冒烟与它们同级、是硬门禁：在 Xvfb 里真渲染一帧，再断言 PNG 非空白，红了就不许合。macOS 探针还不算
——只手动 dispatch，结果留证据但不作为流水线结论。

| Workflow | 内容 |
|----------|------|
| `.github/workflows/ci.yml` | fmt + clippy + test + release build（Windows / Linux 矩阵）；Linux runtime smoke（Xvfb 真渲染一帧、断言 PNG 非空白）；macOS 运行期探针（仅手动 dispatch） |
| `.github/workflows/guards.yml` | `crates/core` / `crates/presenter` / `crates/design` 零 UI 守门、单文件 ≤400 行守门、cargo-deny 许可与来源守门 |
| `.github/workflows/release.yml` | tag `v*` → 六槽位构建 → 单文件产物 + `SHA256SUMS.txt` → GitHub Release |

## 架构与模块地图

```
Unlucky-Launcher (cargo workspace)
├── crates/core       # 领域库：零 UI 依赖，八模块
├── crates/agent      # AI Agent：ToolSpec 注册表 + 内置客户端 + MCP server（双形态共用一个内核）
├── crates/presenter  # 业务宿主层：与框架无关的 state / host / dto / 转换
├── crates/design     # 设计令牌：真 HCT 色彩 / 排版 / 形状 / 间距 / 高程 / 动效 / 图标 / 组件度量
│                     #   —— 零 UI 依赖、零第三方 crate
├── crates/i18n       # 语言探测、消息目录、中文字体策略
├── crates/palette-extract  # 唯一解码图片的 crate，供壁纸取色链路使用
├── crates/app-iced   # iced UI（bin `unlucky-launcher-iced`）：workspace 里唯一的 GUI bin
├── crates/cli        # CLI（bin `ull`）：headless，零 UI
└── packaging         # 单文件产物矩阵与发布脚本
```

层间纪律：`app-iced` → `presenter` → `core` / `agent`。`design` 与 `i18n` 只被壳层与 presenter 消费；
它们都不得反过来依赖渲染框架，CI 守门会拦。

`crates/core` 的八个模块：

| 模块 | 职责 |
|------|------|
| `setting` | 三模式数据目录、schema 配置、原子写、迁移 |
| `download` | 多源健康度、镜像、分块下载、边下边算 |
| `game` | 版本/实例仓库、快照与继承解析 |
| `launch` | 10 阶段启动管线、进程、JSONL 事件流 |
| `auth` | Microsoft 账户登录（OAuth 2.0 device code → Xbox Live → XSTS → Minecraft Services）、第三方社区服、本地档案 |
| `addon` | 加载器安装、整合包、CurseForge / Modrinth、整合包导出 |
| `diagnose` | 五类信号采集、YAML 规则引擎、hs_err 解析 |
| `platform` | OS 抽象：路径、进程、文件锁、能力探测、运行期 OS 检测 |

## 许可证与致谢

- **本项目**：Apache License 2.0，见 [LICENSE](LICENSE)；第三方声明见 [NOTICE](NOTICE)。
- **iced**（<https://github.com/iced-rs/iced>），MIT：现役 UI 的框架；与它同栈的 `winit`（Apache-2.0）、
  以及经它引入的 `wgpu`（MIT / Apache-2.0）/ `tiny-skia`（BSD-3-Clause）。
- **Google Material Color Utilities**（Apache-2.0）：`crates/design` 的色彩令牌是它 HCT /
  ColorSpec2021 算法的 Rust 重写移植，移植清单与逐函数对照写在 [NOTICE](NOTICE)。
- **其余主要依赖**：MCP server 的 `rmcp` 与 `axum`（Apache-2.0 / MIT）、`tokio`（MIT）、`reqwest`
  与 rustls（MIT / Apache-2.0）、`serde` / `serde_json`（MIT / Apache-2.0）、`zip` / `flate2` / `tar`
  （MIT / Apache-2.0）。cargo-deny 强制执行的白名单在 `deny.toml`；GPL 系依赖不得进主干。

感谢 [HMCL](https://github.com/HMCL-dev/HMCL) 和 [PCL2](https://github.com/Meloong-Git/PCL)
（均 GPL-3.0）。这两款启动器陪着绝大多数 MC 玩家走了很多年，
本项目在设计取舍与实现细节上都从中受益良多。
