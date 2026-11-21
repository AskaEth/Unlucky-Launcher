# 日志在哪（M188）

一句话：**启动器自己写的东西全在 `<数据根>/logs/` 一个目录里**，游戏自己的东西在实例运行目录里。
设置页那行「日志文件夹」显示的就是前者的绝对路径，旁边有 *打开文件夹* 与 *复制路径*；日志页页头
也有 *打开文件夹*。两个入口走同一个落点（`crates/app-iced/src/app/logs_folder.rs`），失败时输出的
文本带完整路径。

## 1. 三个数据模式下的绝对路径

`<数据根>` 由 `core::platform::data_dir` 的三模式解析（便携 / 标准 / 自定义）：

| 模式 | 启动器数据目录（= `<数据根>`） | 日志根 |
|---|---|---|
| portable | `<exe 目录>/.ULL` | `<exe 目录>/.ULL/logs` |
| standard | Windows `%APPDATA%\ULL` · macOS `~/Library/Application Support/ULL` · Linux `$XDG_DATA_HOME/ULL`（缺省 `~/.local/share/ULL`） | `<上者>/logs` |
| custom | `<自定义根>/.ULL` | `<自定义根>/.ULL/logs` |

GUI 的 `--data-dir <dir>` 是 custom 模式的一种入口：那时日志根是 `<dir>/.ULL/logs`。设置页显示的
路径与 `launcher.log` 的实际落点取的是**同一份解析结果**（`SettingsBridge` → `<home>/config/launcher.json`
→ `<home>`），所以两者不会各说各话。

## 2. 目录里有什么

| 路径 | 写者 | 是什么 |
|---|---|---|
| `logs/launcher.log` | `core::launch::launcher_log` | 启动器自己的运行日志：启动拍（主题 / 语言 / 路由 / 窗口）、后端探测与降级、字体链、每一次启动的命令行事实 / spawn 结果 / 退出码、打开日志目录等用户动作的结果。 |
| `logs/launcher.1.log` | 同上（滚动） | 上一份日志。**只留一份**历史。 |
| `logs/game/<session-id>.jsonl` | `core::launch::session` | 每次启动一份 JSONL 事件流（命令行已脱敏、每行 stdout/stderr、退出、诊断）。 |
| `logs/game/index.jsonl` | `core::launch::session_index` | 一会话一行摘要的索引，日志页左栏读它。 |
| `logs/install.log` | `presenter::host::install_journal` | 安装流水，每阶段一行。 |
| `logs/session-log-location.json` | `presenter::host::logs::note_location` | 会话日志现在在哪 / 以前在哪 / 搬没搬（`moved: 0`），每次启动重写。 |
| `data/agent/sessions/game/…` | **旧版本** | 只读兼容，见 §4。 |

### 上限与滚动

- 单文件上限 `launcher_log::MAX_BYTES = 1 MiB`；每次追加前检查，超过就把 `launcher.log` 改名成
  `launcher.1.log`（旧的那份被替换），新文件从 0 开始。
- 保留份数：**1**。所以 `logs/` 里启动器日志的稳态占用 ≤ 约 2 MiB（+ 同目录的会话 jsonl）。
- 为什么不做按日期滚动：事故复现往往在「刚刚那一次」，份数比历史长度更重要；而无限份数会让「导出
  诊断包」变成搬砖。

## 3. 哪些行进 launcher.log、哪些只进 stderr

判据（复审后收紧成这一句）：**逐帧度量不接；显式取证开关与一次性事故行可接**。实现是
`crates/app-iced/src/app/diag.rs` 的 `diag!` 宏（stderr + 文件双写，行首 `[tag]` 就是日志里的 scope）。

| 进 launcher.log | 只进 stderr |
|---|---|
| 启动编排：`[probe]`（`request=… -> ICED_BACKEND=…`、降级重开、探测耗时）、`[font]`、`[theme]`、`[appearance]`、`[gpu]` 的降级与拒绝降级、`[boot]`（含后端 request/effective）、`[shot]` 的通道提示 | 验收/出图矩阵的**逐帧度量**：`[window]` / `[perf]` / `[caps]` / `[theme] source=…`（`renderer::acceptance/**`，一次启动上百行，会冲掉事故现场） |
| **一次性事故行**：`[gpu] <backend> 初始化失败（具名）` 与 `[gpu] 失败详情（具名）`（`renderer::report_failure`，复审 P2-1）、`[gpu] <target>: …Selected: AdapterInfo…`（`renderer/log_bridge.rs` 转出的 wgpu 日志）、`[gpu-preflight]`（`ULL_PREFLIGHT=1`） | 组件几何探针的断言输出（`components/**` 的 `[feedback-probe]`、gallery 页面里的 `[gallery]`） |
| 用户动作的结果：`[logs]`（打开日志文件夹成功/失败）、`[settings]` 写盘失败、`[i18n]` 语言没落盘、`[update]` 的忽略/重启、`[agent]` / `[content]` / `[page]` 的落点提示 | 每条日志行的正文里再套的括号（不是标签） |
| 显式开关下的一次性行：`[tick]`（`ULL_TICK_TRACE=1`，≈1 行/秒，约 4.8 小时才滚一次）、`[corner]`（`app/update/appearance.rs` 打，`platform/win32/corner.rs` 只负责 `format_line`）、`[appearance]` / `[theme]` 的切换 | — |

`[corner]` 到底在哪一侧，看**打印点**：唯一打印点是 `app/update/appearance.rs`（走 `diag!`），
`platform/win32/corner.rs::format_line` 只拼字符串、不打印 ⇒ 它**进** launcher.log。

## 3.1 还没接进日志的行（本 mission 之外）

- `renderer::acceptance/**` 的 `[window]` / `[perf]` / `[caps]`：逐帧/矩阵取证，按上面的判据**故意不接**。
- `components/**` 的几何探针（`[feedback-probe]` / `[gallery]`）：测试通道，不进产品日志。

（`renderer/mod.rs` 的 `report_failure`、`renderer/log_bridge.rs`、`renderer/preflight.rs`、
`probe.rs:apply_with_preflight` 已在 M188 复审后接上。）

## 4. 会话日志搬家：策略是**不搬**

M188 把新会话从 `data/agent/sessions/game/` 挪到 `logs/game/`（目录名里的 `agent` 是历史遗留，
它从来只装游戏会话）。旧目录的处理方式：

- **原样保留、继续读**：`presenter::host::logs` 与 `cli` 的会话读取都是两个根合并，同一个
  `session_id` 同时存在时以 `logs/` 那条为准；
- **不搬、不删**：搬运用户已有数据是不可逆动作，中途失败会留下半份会话，崩溃弹窗里的会话关联也会断；
- **留记录**：`logs/session-log-location.json` 写明两个根、旧根在不在、旧根里几个文件、`moved: 0`、
  `policy: "read-only-compat"`。

## 5. 游戏自己的日志（不归启动器写）

在实例的运行目录里（隔离实例是 `<游戏目录>/versions/<隔离名>/…`，共享实例直接是 `.minecraft`）：

- `logs/latest.log`、`logs/<日期>-<序号>.log.gz` —— 游戏自己的日志；
- `crash-reports/crash-*.txt` —— **只在 JVM 活到崩溃处理器那一步时才有**；
- `hs_err_pid*.log` —— JVM 自己崩（native crash）时才写。

模组 / 缺库 / Java 选错把游戏弄死在 Minecraft 初始化之前时，上面三个都不会出现——那不是启动器
丢了日志，而是游戏没机会写。那种时候的证据是 `logs/game/<session-id>.jsonl` 与 `logs/launcher.log`。

## 6. 没验的部分

- **真机「双击启动后日志确实在」**：本轮只在开发机上用 `cargo test` 与本地进程验证，Windows 桌面
  双击启动需要用户实测。
- **macOS / Linux 的日志根**：路径由同一条解析链给出（`standard_launcher_home` 有单测覆盖三个平台），
  但「Mac 上点打开文件夹真的会开 Finder」「Linux 上没有 `xdg-open` 时会怎样」没有真机验证。
- **只读数据目录**：日志写不进去时按设计静默放过（best-effort），此时设置页显示的路径仍然正确，
  但文件不存在。
