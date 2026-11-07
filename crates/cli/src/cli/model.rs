//! 命令模型（纯数据）：解析结果就是这里的一个 [`Invocation`]。

use std::path::PathBuf;

/// 全局选项（所有命令共享）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Globals {
    /// `--json`：机器可读输出。
    pub json: bool,
    /// `--no-color`：关闭颜色。v1 输出本身不含 ANSI，参数保留以兼容脚本。
    pub no_color: bool,
    /// `--quiet`：静默人类可读输出（错误仍进 stderr）。
    pub quiet: bool,
    /// `--data-dir <dir>`：覆盖数据根（等价 custom 模式的 `custom_root`）。
    pub data_dir: Option<PathBuf>,
}

/// 一条命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// 无子命令：版本摘要 + 帮助（用户原话「无子命令时 --version + --help」）。
    Welcome,
    /// 只打印版本 + 系统摘要。
    Version,
    /// 帮助（`Some(topic)` 给该子命令的帮助）。
    Help(Option<String>),
    /// `instances list`。
    InstancesList,
    /// `install <version> [--loader X] [--name N]`。
    Install(InstallArgs),
    /// `pack import <file> [--target <dir>] [--name N] [--overwrite] [--no-register]`。
    PackImport(PackImportArgs),
    /// `launch <instance> [--dry-run]`。
    Launch(LaunchArgs),
    /// `sessions list [--limit N]`。
    SessionsList {
        /// 最多返回几条。
        limit: usize,
    },
    /// `logs <sid> [--tail N]`。
    Logs(LogsArgs),
    /// `diag <sid>`。
    Diag(DiagArgs),
    /// `config get [<key>]`。
    ConfigGet(ConfigGetArgs),
    /// `config set <key> <value>`。
    ConfigSet(ConfigSetArgs),
    /// `java list`。
    JavaList,
    /// `java install <major> [--vendor X]`。
    JavaInstall {
        /// 目标 JDK 主版本（8 / 17 / 21）。
        major: u32,
        /// 钉死的发行版（缺省走 D23 的回退链）。
        vendor: Option<unlucky_core::game::java::JdkVendor>,
    },
    /// `java vendors`：列出各发行版的可用性 / 镜像 / 当前优先级。
    JavaVendors,
    /// `accounts list`。
    AccountsList,
    /// `accounts add <kind> [<name>]`。
    AccountsAdd(AccountsAddArgs),
    /// `accounts remove <name>`。
    AccountsRemove {
        /// 账户名（或条目 id）。
        name: String,
    },
    /// `agent serve`：对外 MCP server（长驻）。
    AgentServe(AgentServeArgs),
    /// `agent config`：打印 Claude Code / Cursor 的客户端配置样例。
    AgentConfig {
        /// 覆盖端点端口（默认取设置键 `agent.mcp.port`）。
        port: Option<u16>,
    },
}

/// `agent serve` 的参数（命令行只做覆盖，默认值来自设置键 `agent.mcp.*`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentServeArgs {
    /// 走 stdio 形态（llama.cpp 内置 MCP client 用）；缺省是 Streamable HTTP。
    pub stdio: bool,
    /// 覆盖端口。
    pub port: Option<u16>,
    /// 覆盖监听地址（只接受回环地址，非回环会被 server 拒绝）。
    pub host: Option<String>,
    /// 本次运行放开写工具（仍需设置键 `agent.mcp.allow_write` 或本 flag 之一）。
    pub allow_write: bool,
}

/// `accounts add` 的轨道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountsAddKind {
    /// 离线（`add offline <name>`）。
    Offline,
    /// 微软 device-code（`add msa`，交互式）。
    Microsoft,
}

impl AccountsAddKind {
    /// 稳定名（帮助文案与 JSON 输出用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Microsoft => "msa",
        }
    }
}

/// `accounts add` 的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountsAddArgs {
    /// 轨道。
    pub kind: AccountsAddKind,
    /// 离线轨的玩家名（微软轨不需要）。
    pub name: Option<String>,
}

/// `install` 的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallArgs {
    /// 目标版本 id（`1.20.1` / `1.20.1-forge-47.2.0`）。
    pub version: String,
    /// 加载器（`vanilla` 缺省 / `fabric` / `quilt` / `neoforge` / `forge`）。
    pub loader: Option<String>,
    /// 加载器版本（缺省取官方默认：Fabric 系首个稳定版，maven 系该原版下最新）。
    pub loader_version: Option<String>,
    /// 实例显示名（缺省用版本 id）。
    pub name: Option<String>,
    /// 只出安装计划（列清单 + 总大小 + 源选择），不下载、不登记。
    pub dry_run: bool,
    /// 是否往 stderr 打实时进度（`--quiet` 下关掉）。
    pub progress: bool,
    /// 实时进度用 JSONL 形态（`--json` 时开，stderr 一行一条，stdout 仍是单个信封）。
    pub json_progress: bool,
}

/// `pack import` 的参数（M111）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackImportArgs {
    /// 整合包路径：`.zip` / `.mrpack` / MultiMC 实例目录。
    pub path: PathBuf,
    /// 目标实例目录（缺省 = 数据根的共享游戏目录 `game_home`）。
    pub target: Option<PathBuf>,
    /// 实例显示名（缺省 = 包名）。
    pub name: Option<String>,
    /// 冲突时用包内内容覆盖（缺省保留用户文件）。
    pub overwrite: bool,
    /// 不登记实例（只落内容）。
    pub no_register: bool,
}

/// `launch` 的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchArgs {
    /// 实例 id 或实例名；也可以直接给版本 id。
    pub instance: String,
    /// 只组装命令行，不 spawn（D18 的 `--dry-run`）。
    pub dry_run: bool,
    /// Java 可执行文件覆盖。
    pub java: Option<PathBuf>,
    /// 离线账户名（缺省 `Player`）。
    pub username: Option<String>,
    /// 账户名（`accounts` 里的玩家名或条目 id）；缺省用唯一账户。
    pub account: Option<String>,
    /// 最大堆（MB）。
    pub max_memory: Option<u32>,
}

/// `logs` 的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogsArgs {
    /// 会话 id（允许唯一前缀）。
    pub session_id: String,
    /// 只打印末尾 N 条事件（缺省 200）。
    pub tail: usize,
}

/// `diag` 的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagArgs {
    /// 会话 id（允许唯一前缀）。
    pub session_id: String,
    /// 送入规则引擎的日志行上限（缺省 2000）。
    pub tail: usize,
}

/// `config get` 的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigGetArgs {
    /// 点分键路径；`None` 表示整份生效配置。
    pub key: Option<String>,
    /// 敏感键也输出明文（默认脱敏）。
    pub show_secret: bool,
}

/// `config set` 的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSetArgs {
    /// 点分键路径。
    pub key: String,
    /// 值的原始文本（先按 JSON 解析，失败按字符串）。
    pub value: String,
}

/// 一次调用的完整输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// 全局选项。
    pub globals: Globals,
    /// 要执行的命令。
    pub command: Command,
}
