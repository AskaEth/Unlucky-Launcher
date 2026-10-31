//! 命令行组装总表：`analyse/02` §4.1 的 33 项顺序表的 Rust 化。
//!
//! 顺序本身有语义（后加的默认值会被先加的显式值抑制）：
//!
//! | # | 内容 | 本文件落点 |
//! |---|------|-----------|
//! | 1~3 | `nice` / 用户包装器 / java 可执行文件 | [`assemble`] |
//! | 4 | 最高优先级 JVM 参数（含 `@argfile` 展开） | [`assemble`] |
//! | 5~7 | `-Xmx`/`-Xms`、Metaspace、用户 JVM 参数（按默认值） | [`assemble`] |
//! | 8~17 | 编码、Log4j、主 jar、代理、自动优化、版本开关、FML、渲染器 | `jvm_args` |
//! | 18~19 | classpath 计算与占位符表（`${classpath}` 等 26 项） | [`assemble`] + `placeholders` |
//! | 20~32 | 清单 JVM 参数、主类、游戏参数、QuickPlay、代理、图形后端、用户参数 | `game_args` |
//! | 33 | 删除被禁止参数（`-Xincgc`） | [`assemble`] |
//!
//! 组装是**纯函数**：给定 `LaunchRequest` + `PlatformCtx` 一定得到同样的命令行，
//! 因此可以整体快照测试（`crates/core/tests/launch/cmdline_snapshot.rs`）。

use std::fmt;
use std::path::PathBuf;

use crate::launch::command_builder::{quote_for_display, CommandBuilder};
use crate::launch::defaults;
use crate::launch::error::{AssemblyError, Result};
use crate::launch::game_args;
use crate::launch::jvm_args;
use crate::launch::placeholders::{join_classpath, PlaceholderMap};
use crate::launch::platform::PlatformCtx;
use crate::launch::request::{LaunchRequest, MemoryOptions};

/// 组装过程中的“非致命”告警（不中断启动，但要告诉用户我们改了什么）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssemblyWarning {
    /// 注入了 log4j 参数却没有配置文件。
    Log4jConfigMissing,
    /// 用户自己写了代理参数，我们跳过自动代理参数。
    ProxyJvmArgsSkipped,
    /// `-Xms` 大于 `-Xmx`，已忽略 `-Xms`。
    MemoryMinIgnored {
        /// 请求的最小堆。
        min_mb: u32,
        /// 请求的最大堆。
        max_mb: u32,
    },
    /// 版本不支持 QuickPlay，已退化成 `--server/--port`。
    QuickPlayDegraded {
        /// 原始地址。
        address: String,
    },
    /// 服务器地址非法，QuickPlay 被丢弃。
    QuickPlayAddressInvalid {
        /// 原始地址。
        address: String,
    },
    /// 版本不支持 QuickPlay 且无法退化（存档/Realms）。
    QuickPlayUnsupported {
        /// 游戏版本。
        version: String,
    },
    /// 版本用不了 `--graphicsBackend`，已丢弃。
    ///
    /// 两种来源：可解析版本低于 `26.2-snapshot-2`；或周快照（`26w14a`）但版本库表里
    /// 没有 `lwjgl-vulkan`（见 `game_args::graphics_backend_allowed`）。
    GraphicsBackendSkipped {
        /// 游戏版本。
        version: String,
    },
}

impl fmt::Display for AssemblyWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Log4jConfigMissing => f.write_str("log4j 注入已启用但缺少配置文件，已跳过"),
            Self::ProxyJvmArgsSkipped => f.write_str("用户已指定代理参数，跳过自动代理 JVM 参数"),
            Self::MemoryMinIgnored { min_mb, max_mb } => {
                write!(f, "-Xms{min_mb}m 大于 -Xmx{max_mb}m，已忽略 -Xms")
            }
            Self::QuickPlayDegraded { address } => {
                write!(f, "版本不支持 QuickPlay，已退化为 --server {address}")
            }
            Self::QuickPlayAddressInvalid { address } => {
                write!(f, "服务器地址非法，已丢弃 QuickPlay：{address}")
            }
            Self::QuickPlayUnsupported { version } => {
                write!(f, "版本 {version} 不支持该 QuickPlay 目标，已丢弃")
            }
            Self::GraphicsBackendSkipped { version } => {
                write!(f, "版本 {version} 不支持 --graphicsBackend，已丢弃")
            }
        }
    }
}

/// 组装结果。
#[derive(Debug, Clone)]
pub struct AssembledCommand {
    /// 实际要 spawn 的程序（通常是 java，带包装器时是包装器）。
    pub program: PathBuf,
    /// 参数（不含 `program`）。
    pub args: Vec<String>,
    /// 完整 classpath（库 + 主 jar 末位）。
    pub classpath: Vec<PathBuf>,
    /// 日志解码编码（决定 pump 用什么解码子进程输出）。
    pub encoding: String,
    /// 内存设置（与 `-Xmx`/`-Xms` 一致）。
    pub memory: MemoryOptions,
    /// 环境变量（实例标识/渲染/加载器/用户自定义）。
    pub env: Vec<(String, String)>,
    /// 组装告警。
    pub warnings: Vec<AssemblyWarning>,
    /// 主类。
    pub main_class: String,
}

impl AssembledCommand {
    /// 完整 argv（`program` 在第一个）。
    pub fn argv(&self) -> Vec<String> {
        let mut argv = Vec::with_capacity(self.args.len() + 1);
        argv.push(self.program.display().to_string());
        argv.extend(self.args.iter().cloned());
        argv
    }

    /// 人读形式（含空格的参数加引号），用于日志与快照测试。
    pub fn display(&self) -> String {
        let mut parts = Vec::with_capacity(self.args.len() + 1);
        parts.push(quote_for_display(&self.program.display().to_string()));
        for arg in &self.args {
            parts.push(quote_for_display(arg));
        }
        parts.join(" ")
    }
}

/// 组装完整命令行。
pub fn assemble(req: &LaunchRequest, ctx: &PlatformCtx) -> Result<AssembledCommand> {
    let main_class = req
        .manifest
        .main_class
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AssemblyError::MissingMainClass)?
        .to_string();
    if req.java.binary.as_os_str().is_empty() {
        return Err(AssemblyError::EmptyJavaPath.into());
    }
    if !req.missing_components.is_empty() {
        return Err(AssemblyError::MissingComponents(req.missing_components.clone()).into());
    }
    let classpath = effective_classpath(req);
    if classpath.is_empty() {
        return Err(AssemblyError::EmptyClasspath.into());
    }

    // 第 19 项：占位符表（`${classpath}` 用最终 classpath，主 jar 已在末位）。
    let mut placeholders = PlaceholderMap::from_request(req, ctx);
    placeholders.insert(
        "classpath",
        join_classpath(&classpath, ctx.path_separator()),
    );

    // 第 1~3 项：优先级包装 → 用户包装器 → java 可执行文件。
    let mut head: Vec<String> = Vec::new();
    if let Some(nice) = req.nice {
        if ctx.os.is_posix() {
            head.push("nice".to_string());
            head.push("-n".to_string());
            head.push(nice.to_string());
        }
    }
    head.extend(req.wrapper.iter().cloned());
    head.push(req.java.binary.display().to_string());
    let program = PathBuf::from(head.remove(0));

    let mut builder = CommandBuilder::with_placeholders(placeholders);
    for token in &head {
        builder.add_without_parsing(token.clone());
    }

    // 第 4 项：最高优先级 JVM 参数（原样追加，且登记 @argfile 展开结果）。
    builder.add_all_without_parsing_and_read_external(
        &req.user.override_jvm,
        &crate::launch::command_builder::read_argfile,
    );

    let mut warnings = Vec::new();

    // 第 5 项：`-Xmx` / `-Xms`（`-Xms` 必须 ≤ `-Xmx`）。
    if let Some(max_mb) = req.memory.max_mb {
        let _ = builder.add_default("-Xmx", &format!("{max_mb}m"));
    }
    if let Some(min_mb) = req.memory.min_mb {
        match req.memory.max_mb {
            Some(max_mb) if min_mb > max_mb => {
                warnings.push(AssemblyWarning::MemoryMinIgnored { min_mb, max_mb });
            }
            _ => {
                let _ = builder.add_default("-Xms", &format!("{min_mb}m"));
            }
        }
    }

    // 第 6 项：Metaspace（Java ≥8）或 PermSize（更早）。
    if req.java.major >= 8 {
        let _ = builder.add_default("-XX:MetaspaceSize=", "64m");
    } else {
        let _ = builder.add_default("-XX:PermSize=", "64m");
    }

    // 第 7 项：用户 JVM 参数——整体按“默认值”处理。
    builder.add_all_default(&req.user.jvm);

    // 第 8~17 项。
    let (encoding, jvm_warnings) = jvm_args::apply_jvm_arguments(req, ctx, &mut builder);
    warnings.extend(jvm_warnings);

    // 第 20~32 项。
    warnings.extend(game_args::apply_game_arguments(req, ctx, &mut builder));

    // 第 33 项：删除禁用参数。
    builder.remove_forbidden(&defaults::forbidden_args(req.java.major));

    let env = defaults::environment(req, ctx.os);
    Ok(AssembledCommand {
        program,
        args: builder.into_args(),
        classpath,
        encoding,
        memory: req.memory,
        env,
        warnings,
        main_class,
    })
}

/// 最终 classpath：库按给定顺序去重，主 jar 永远追加在末位（Mojang 惯例，
/// 影响同名类优先级，`analyse/02` §4.5）。
pub fn effective_classpath(req: &LaunchRequest) -> Vec<PathBuf> {
    let mut classpath: Vec<PathBuf> = Vec::new();
    let candidates = req
        .classpath
        .iter()
        .chain(std::iter::once(&req.dirs.primary_jar));
    for path in candidates {
        if path.as_os_str().is_empty() {
            continue;
        }
        if !classpath.contains(path) {
            classpath.push(path.clone());
        }
    }
    classpath
}
