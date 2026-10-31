//! 帮助文案（`ull --help` / `ull help <命令>`）。
//!
//! 命令集与 `10-cli-design.md` §3 对齐；`agent serve|config`（M39 的 MCP server）已列，
//! `agent ask`（内置对话的 headless 形态）仍未列。

/// 总帮助。
#[must_use]
pub const fn general() -> &'static str {
    "\
ull · Unlucky-Launcher 命令行（D18）

用法:
  ull [全局选项] <命令> [参数]

命令:
  instances list                      列出实例与已安装版本
  install <version> [--loader X] [--loader-version V] [--name N] [--dry-run]
                                      下载并安装版本本体（--loader fabric|quilt|neoforge|forge
                                      时再叠一层加载器）→ 登记实例；--dry-run 只出计划
  launch <instance> [--dry-run] [--account <name>] [--java <path>] [--username <name>]
                   [--max-memory <MB>] 启动实例；--dry-run 只打印组装后的命令行
  accounts list                       列出账户（名称 / 轨道 / UUID，**不含凭据**）
  accounts add offline <name>         添加离线账户（HMCL 兼容 UUID）
  accounts add msa                    添加微软账户（设备码交互式登录）
  accounts remove <name>              删除账户（连同凭据）
  sessions list [--limit N]           列出会话记录（倒序）
  logs <sid> [--tail N]               打印会话事件流末尾
  diag <sid> [--tail N]               对会话跑规则引擎诊断
  config get [<key>] [--show-secret]  读配置（生效值；无 key 时输出整份）
  config set <key> <value>            写配置（走 setting schema 校验）
  java list                           列出可用 Java（含每个 JDK 的实际落点）
  java install <major> [--vendor X]   安装托管 JDK（缺省按地区回退链选发行版；
                                      --vendor temurin|oracle|microsoft|zulu 钉死一家）
  java vendors                        列出四家发行版的镜像/支持版本/当前优先级
  agent serve [--stdio] [--port N]    起对外 MCP server（默认 127.0.0.1 的 Streamable HTTP；
                [--allow-write]       --stdio 给 llama.cpp 内置 client 用；默认只读）
  agent config [--port N]             打印 Claude Code / Cursor 的配置样例
  version                             版本 + 系统摘要（同 --version）
  help [命令]                         本帮助

全局选项:
  --json          机器可读输出：成功 {\"ok\":true,\"data\":…}，失败 {\"ok\":false,\"error\":…}
  --data-dir P    覆盖数据根（等价 custom 模式的根目录，即含 .ULL/ 与 .minecraft/ 的父目录）
  --no-color      关闭颜色（v1 输出不含 ANSI，参数保留以兼容脚本）
  --quiet         静默人类可读输出（错误与提示仍进 stderr）
  --help, -h      帮助（有位置参数时按该命令给帮助）
  --version, -V   版本 + 系统摘要

退出码: 0 成功 / 1 用户错误 / 2 内部错误"
}

/// 单个命令的帮助；未知主题返回 `None`（调用方退回总帮助）。
#[must_use]
pub fn topic(name: &str) -> Option<&'static str> {
    match name {
        "instances" => Some(
            "\
ull instances list

列出 profiles.json 里的实例与 versions/ 下已安装的版本。
 --json 输出 {\"data\":{\"instances\":[…],\"installed_versions\":[…],\"data_root\":…}}",
        ),
        "install" => Some(
            "\
ull install <version> [--loader X] [--loader-version V] [--name N] [--dry-run]

解析版本清单与继承链 → 组下载计划 → 落盘（versions/ assets/ libraries/）→ 登记实例
（profiles.json + config/instances/<id>.json）。已存在且校验通过的文件会跳过，
重复装同一版本不重复下载、不重复建实例。
--dry-run：只输出计划（文件清单 + 总大小 + 每类资源的源选择），不下载、不登记。
--loader：vanilla（缺省）/ fabric / quilt / neoforge / forge。装加载器时先生成
versions/<原版>-<加载器>-<版本>/<…>.json（inheritsFrom 原版 + 主类 + loader 库），
原版没装会先装原版，实例的 versionId 指向 loader 版本。
--loader-version：加载器版本；缺省取官方默认（Fabric 系首个稳定版，maven 系最新）。
  Forge 可写 47.2.0 或 1.20.1-47.2.0 两种形态。
  NeoForge/Forge 里需要跑官方 installer processors 的版本会真跑一遍（M35 已实装）：
  缺 Java 时按 D23 的回退链自动装一个托管 JDK；--dry-run 只列出要跑的 processor 与依赖库。
--json：stdout 是单个结果信封 {\"data\":{\"plan\":…,\"result\":…,\"instance\":…}}，
实时进度改走 stderr 的 JSONL（一行一条 {\"event\":\"progress\"|\"file_failed\"}）；
--quiet 时连进度也不打。",
        ),
        "launch" => Some(
            "\
ull launch <instance> [--dry-run] [--account <name>] [--java <path>] [--username <name>]
            [--max-memory <MB>]

<instance> 可以是实例 id、实例名，或直接是版本 id。
--dry-run（D18）：只组装命令行并打印，不 spawn 进程、不写会话文件、不刷新令牌。
账户：--account <name> 指定账户（玩家名或条目 id）；不给时用唯一账户，一个都没有则回落
离线占位（--username 或 Player），有多个账户会报错并列出候选。
accessToken 一律脱敏（输出里出现的是 <redacted:token>）。",
        ),
        "accounts" => Some(
            "\
ull accounts list
ull accounts add offline <name>
ull accounts add msa
ull accounts remove <name>

账户存在 <数据根>/.ULL/data/auth/：accounts.json（可公开元数据）与 credentials.dat（凭据）
**分文件**存放，先写凭据后写元数据、原子写、凭据文件 POSIX 侧 0600。
凭据永不打印：list 只输出名称/轨道/UUID。
add msa 走微软 device-code（用户码打在 stderr，去 https://www.microsoft.com/link 输码），
需要先配置 ULL_MSA_CLIENT_ID（Azure 公共客户端 id）。
authlib-injector 账户的密码登录留给 GUI（密码进 argv 会泄露，CLI 不开这条通道）。",
        ),
        "sessions" => Some("ull sessions list [--limit N]  列出会话索引（按开始时间倒序，缺省 20 条）"),
        "logs" => Some("ull logs <sid> [--tail N]  <sid> 允许唯一前缀；输出会话 JSONL 末尾 N 条事件（缺省 200）"),
        "diag" => Some(
            "ull diag <sid> [--tail N]  把会话的退出信号与日志喂给规则引擎（内嵌规则集），输出结构化诊断",
        ),
        "config" => Some(
            "\
ull config get [<key>] [--show-secret]
ull config set <key> <value>

读写 <数据根>/.ULL/config/launcher.json，走 core::setting 的 schema（类型/枚举校验、
原子写）。敏感键 get 时脱敏，--show-secret 才给明文；值先按 JSON 解析，失败按字符串。",
        ),
        "java" => Some(
            "\
ull java list                           探测 JAVA_HOME / PATH / 托管目录 / 实例覆盖里的 Java
                                        并打印每个 JDK 的实际落点（managed 项带目录）
ull java install <major> [--vendor X]   安装托管 JDK（D13/D23）：回退链查版本 → 下载 → 解压 → 验证
ull java vendors                        列出四家发行版的镜像/支持版本/当前优先级顺序

落点（D23 三模式，随 core.data_mode 与设置键 java.installRoot 走）：
  portable（默认）  <数据根>/.ULL/data/java-managed/jdk-<major>-<vendor>/
  standard          Windows %APPDATA%\\Java\\<发行方>\\<版本>\\；macOS ~/Library/Application Support/Java/…
                    Linux $XDG_DATA_HOME/java/…
  custom            设置 java.installRoot 后按 <root>/<发行方>/<版本>/（覆盖前两者）

发行版回退链（D23）：国内 temurin(TUNA) → oracle(华为云→南大) → microsoft(aka.ms)
→ zulu；国外 temurin → microsoft → oracle → zulu。设置 java.vendorPriority 可改顺序
（只写几家也行，其余按默认序补齐）；--vendor 钉死一家、不自动降级。
每家失败的原因（版本不存在 / 镜像全 4xx / 校验失败）进 --json 的 chain 块与人读的「降级：」行。",
        ),
        "agent" => Some(
            "\
ull agent serve [--stdio] [--port N] [--host H] [--allow-write|--readonly]
ull agent config [--port N]

把 agent 的工具注册表挂成 **MCP server**（02 文档 §1 的第二形态：同一注册表、同一安全策略、
同一审计）。默认走 Streamable HTTP，只绑回环地址（127.0.0.1，默认端口 47823）；
--stdio 是给 llama.cpp server 的内置 MCP client 用的形态。

写权限默认**关**（只读子集）：要放开需设置键 agent.mcp.allow_write=true 或 --allow-write。
敏感键（High 分级）在 MCP 场景**默认拒绝**（外部客户端没有确认弹窗）：让用户去设置页改，
或在设置 → AI 里打开 YOLO（审计照记 yolo=true）。
令牌：设置键 agent.mcp.token；非空时要求 Authorization: Bearer <token>。
`agent config` 直接给出 Claude Code {\"type\":\"http\",\"url\":…} 与 Cursor {\"url\":…} 的样例。",
        ),
        _ => None,
    }
}
