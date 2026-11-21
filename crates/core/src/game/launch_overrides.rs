//! 实例级启动参数覆盖（五项）+ **三级回落**（实例 → 全局 → 默认）。
//!
//! 依据：M157 的定案（叠加/冲突规则见该 mission 的 notes；用户 2026-11-07 拍板实例配置页
//! 支持启动参数，含 MC 26.2+ 的 `--graphicsBackend vulkan`）。
//!
//! # 为什么是「五项全 Option」
//!
//! 每个字段独立回落：只填了内存的实例不能把全局 JVM 参数一起顶掉。`None`（或空白字符串）
//! = 这一项跟随下一层，与 D20 的 `isolation` 同一套稀疏语义（[`super::install::instance`]）。
//!
//! # 键名为什么叫 `launch` 而不是 `overrides`
//!
//! `overrides` 已经被 D20 占用（`["maxMemory", "javaType", "runningDirectory"]` 这份
//! 「该实例覆盖了哪些全局键」的清单），抢占会让存量实例文件的语义漂移。五项**取值**段落
//! 落在 `launch` 对象里，互不干扰。
//!
//! # 三级回落逐项（**不是整段覆盖**）
//!
//! | 字段 | 实例层 | 全局层 | 出厂默认 |
//! |---|---|---|---|
//! | `jvmArgs` | `launch.jvmArgs` | `java.extra_args` | 空 |
//! | `gameArgs` | `launch.gameArgs` | （无全局键，留 `None`） | 空 |
//! | `memoryMb` | `launch.memoryMb` | `java.default_memory_mb` | [`DEFAULT_MAX_MEMORY_MB`] |
//! | `javaPath` | `launch.javaPath` | `java.selected_path` | 自动挑（托管 > JAVA_HOME > PATH） |
//! | `graphicsBackend` | `launch.graphicsBackend` | （无全局键，留 `None`） | `default` |
//!
//! `jvmArgs` 是**追加**而不是替换：全局是基线（进组装表第 7 项 `user.jvm`），实例是精调
//! （进第 4 项 `user.override_jvm`），两者都下发，同名选项 JVM 取后出现者 ⇒ 实例赢。
//! 落位实现在 [`crate::launch::overrides`]，本文件只给**生效值**（纯计算、零 IO）。
//!
//! # 越界的内存值不算「覆盖」
//!
//! 手改文件能绕过 schema 校验，所以 `memoryMb` 在这个模块里也过一遍范围：
//! 越界 ⇒ 当作没填（落到下一层），而不是拿 `-Xmx0m` 去把 JVM 启动弄崩。判据与
//! `setting::instance_keys` 登记的区间同源（[`MIN_MAX_MEMORY_MB`] / [`MAX_MAX_MEMORY_MB`]）。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 出厂默认最大堆（MiB）：与 `setting::keys` 的 `java.default_memory_mb` 默认值一致。
pub const DEFAULT_MAX_MEMORY_MB: u32 = 2048;
/// `memoryMb` 允许的下限（MiB）。
pub const MIN_MAX_MEMORY_MB: u32 = 256;
/// `memoryMb` 允许的上限（MiB）。
pub const MAX_MAX_MEMORY_MB: u32 = 65536;

/// 图形后端三态（任务书：默认 / OpenGL / Vulkan）。
///
/// `Default` **不是**「交给 JVM 决定」的另一个参数，而是「不下发 `--graphicsBackend`」——
/// 26.2 以前根本没有这个参数，下发即 `UnrecognizedOptionException`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsBackend {
    /// 不下发（游戏按自己的默认后端启动）。
    #[default]
    Default,
    /// `--graphicsBackend opengl`。
    Opengl,
    /// `--graphicsBackend vulkan`。
    Vulkan,
}

impl GraphicsBackend {
    /// 全部三态（顺序即 UI 下拉的展示顺序）。
    pub const ALL: [Self; 3] = [Self::Default, Self::Opengl, Self::Vulkan];
    /// 中文档位说明里的稳定 id 之一。
    pub const IDS: [&'static str; 3] = ["default", "opengl", "vulkan"];

    /// 稳定 id（`default` / `opengl` / `vulkan`；GUI 与 `--json` 同词表）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Opengl => "opengl",
            Self::Vulkan => "vulkan",
        }
    }

    /// 从稳定 id 解析；未知值返回 `None`（**调用方必须报错**，不许静默退化成 `default`）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let wanted = raw.trim();
        Self::ALL
            .into_iter()
            .find(|backend| backend.as_str().eq_ignore_ascii_case(wanted))
    }

    /// `--graphicsBackend` 的取值；`Default` = 不下发（`None`）。
    #[must_use]
    pub const fn flag_value(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Opengl => Some("opengl"),
            Self::Vulkan => Some("vulkan"),
        }
    }

    /// 是否需要下发参数（`default` 不需要）。
    #[must_use]
    pub const fn is_default(self) -> bool {
        matches!(self, Self::Default)
    }
}

/// 实例级启动参数覆盖（`config/instances/<id>.json` 的 `launch` 段）。
///
/// 稀疏：只写用户显式改过的项，键不存在即「跟随下一层」。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LaunchOverrides {
    /// 实例级 JVM 参数（空白分隔的文本，支持引号）；空 = 只用全局 `java.extra_args`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jvm_args: Option<String>,
    /// 实例级游戏参数（同上）；空 = 不追加。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_args: Option<String>,
    /// 最大堆（MiB）；空或越界 = 跟随全局 `java.default_memory_mb`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    /// 实例级 Java 可执行文件；空 = 跟随全局 `java.selected_path` 再退到自动挑。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
    /// 图形后端；空 = `default`（不下发 `--graphicsBackend`）。
    ///
    /// 反序列化对未知值**报错**（`unknown variant`），不静默落成 `default`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphics_backend: Option<GraphicsBackend>,
}

impl LaunchOverrides {
    /// 整段是否为空（序列化时用来决定 `launch` 键出不出现）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.normalized() == Self::default()
    }

    /// 归一：空白字符串与空路径当作没填（用户清空输入框 = 回到跟随）。
    #[must_use]
    pub fn normalized(&self) -> Self {
        Self {
            jvm_args: non_blank(self.jvm_args.as_deref()).map(str::to_string),
            game_args: non_blank(self.game_args.as_deref()).map(str::to_string),
            memory_mb: self.memory_mb.filter(|value| valid_memory(*value)),
            java_path: self
                .java_path
                .as_ref()
                .map(|path| path.display().to_string())
                .filter(|text| !text.trim().is_empty())
                .map(PathBuf::from),
            graphics_backend: self.graphics_backend,
        }
    }

    /// 生效的最大堆（已过范围校验；`None` = 这一层没给）。
    #[must_use]
    pub fn effective_memory_mb(&self) -> Option<u32> {
        self.memory_mb.filter(|value| valid_memory(*value))
    }
}

/// 全局层（`config/launcher.json` 里与启动参数相关的键）。
///
/// 读配置归调用方（CLI / presenter 的 host），本结构只承载**值**——`game` 不 import
/// `setting`，回落规则才能纯函数化、可纯单测。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalLaunchDefaults {
    /// `java.extra_args`（基线 JVM 参数）。
    pub jvm_args: Option<String>,
    /// 全局游戏参数（本轮没有对应设置键，留 `None` 占位，回落链照样逐项成立）。
    pub game_args: Option<String>,
    /// `java.default_memory_mb`。
    pub memory_mb: Option<u32>,
    /// `java.selected_path`。
    pub java_path: Option<PathBuf>,
    /// 全局图形后端（本轮没有对应设置键）。
    pub graphics_backend: Option<GraphicsBackend>,
}

impl GlobalLaunchDefaults {
    /// 只带 `java.extra_args` 的最小构造（测试/CLI 常用）。
    #[must_use]
    pub fn with_jvm_args(text: impl Into<String>) -> Self {
        Self {
            jvm_args: Some(text.into()),
            ..Self::default()
        }
    }
}

/// 三级回落之后的**生效值**（拼装链的输入，见 [`crate::launch::overrides::apply`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLaunch {
    /// 全局层 JVM 参数（组装表第 7 项，`None` = 没有）。
    pub global_jvm_args: Option<String>,
    /// 实例层 JVM 参数（组装表第 4 项，最高优先级，`None` = 没有）。
    pub instance_jvm_args: Option<String>,
    /// 游戏参数（第 32 项，最后追加）。
    pub game_args: Option<String>,
    /// 生效最大堆（三级回落 + 范围校验 + 出厂默认，**恒有值**）。
    pub memory_mb: u32,
    /// 生效 Java 可执行文件；`None` = 调用方走自动挑。
    pub java_path: Option<PathBuf>,
    /// 生效图形后端（`Default` = 不下发）。
    pub graphics_backend: GraphicsBackend,
}

impl ResolvedLaunch {
    /// 是否与「全默认」等价（测试与日志用）。
    #[must_use]
    pub fn is_defaults(&self) -> bool {
        self.global_jvm_args.is_none()
            && self.instance_jvm_args.is_none()
            && self.game_args.is_none()
            && self.memory_mb == DEFAULT_MAX_MEMORY_MB
            && self.java_path.is_none()
            && self.graphics_backend.is_default()
    }
}

/// 三级回落：`instance` 为 `None`/空项一律落到 `global`，再落到出厂默认。
#[must_use]
pub fn resolve(
    instance: Option<&LaunchOverrides>,
    global: &GlobalLaunchDefaults,
) -> ResolvedLaunch {
    let instance = instance
        .map(LaunchOverrides::normalized)
        .unwrap_or_default();
    let global = GlobalLaunchDefaults {
        jvm_args: non_blank(global.jvm_args.as_deref()).map(str::to_string),
        game_args: non_blank(global.game_args.as_deref()).map(str::to_string),
        memory_mb: global.memory_mb.filter(|value| valid_memory(*value)),
        java_path: global.java_path.clone(),
        graphics_backend: global.graphics_backend,
    };

    ResolvedLaunch {
        global_jvm_args: global.jvm_args,
        instance_jvm_args: instance.jvm_args,
        // gameArgs 是**替换**语义：实例给了就用实例的（全局层本轮恒空）。
        game_args: instance.game_args.or(global.game_args),
        memory_mb: instance
            .memory_mb
            .or(global.memory_mb)
            .unwrap_or(DEFAULT_MAX_MEMORY_MB),
        java_path: instance.java_path.or(global.java_path),
        graphics_backend: instance
            .graphics_backend
            .or(global.graphics_backend)
            .unwrap_or_default(),
    }
}

/// 内存范围判据（与 `setting::instance_keys` 登记的区间同源）。
#[must_use]
pub fn valid_memory(mb: u32) -> bool {
    (MIN_MAX_MEMORY_MB..=MAX_MAX_MEMORY_MB).contains(&mb)
}

/// 五项**文本**补丁（GUI 表单与 CLI 共用）：`None` = 这一项不改；`Some("")` = 清空回「跟随」。
///
/// 校验只在这里做一次（内存整数/范围、图形后端词表），两个调用端都拿同一条错误文案——
/// 否则「GUI 说合法、CLI 说非法」这类两套真相迟早出现。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOverridesPatch {
    /// 实例层 JVM 参数原文。
    pub jvm_args: Option<String>,
    /// 实例层游戏参数原文。
    pub game_args: Option<String>,
    /// 实例层最大堆（文本形态，见 [`MIN_MAX_MEMORY_MB`] / [`MAX_MAX_MEMORY_MB`]）。
    pub memory_mb: Option<String>,
    /// 实例层 Java 可执行文件。
    pub java_path: Option<String>,
    /// 实例层图形后端（`default` / `opengl` / `vulkan`）。
    pub graphics_backend: Option<String>,
}

impl LaunchOverridesPatch {
    /// 只改图形后端（下拉切换的最小补丁）。
    #[must_use]
    pub fn backend(id: &str) -> Self {
        Self {
            graphics_backend: Some(id.to_string()),
            ..Self::default()
        }
    }

    /// 清空全部五项（`--clear` 的语义；随后可按需再给几项）。
    #[must_use]
    pub fn clear_all() -> Self {
        Self {
            jvm_args: Some(String::new()),
            game_args: Some(String::new()),
            memory_mb: Some(String::new()),
            java_path: Some(String::new()),
            graphics_backend: Some("default".to_string()),
        }
    }
}

impl LaunchOverrides {
    /// 应用文本补丁：`None` 不动、空串清空；非法值**报错**（不静默丢弃）。
    ///
    /// # Errors
    ///
    /// 内存不是整数、越界，或图形后端不在 [`GraphicsBackend::IDS`] 里时返回用户可读文案。
    pub fn patched(&self, patch: &LaunchOverridesPatch) -> Result<Self, String> {
        let mut next = self.normalized();
        if let Some(text) = &patch.jvm_args {
            next.jvm_args = non_blank(Some(text)).map(str::to_string);
        }
        if let Some(text) = &patch.game_args {
            next.game_args = non_blank(Some(text)).map(str::to_string);
        }
        if let Some(text) = &patch.memory_mb {
            next.memory_mb = parse_memory(text)?;
        }
        if let Some(text) = &patch.java_path {
            next.java_path = non_blank(Some(text)).map(PathBuf::from);
        }
        if let Some(text) = &patch.graphics_backend {
            next.graphics_backend = match GraphicsBackend::parse(text) {
                // `default` 是「不下发」而不是要落盘的覆盖值：清成 None（稀疏语义）。
                Some(GraphicsBackend::Default) => None,
                Some(backend) => Some(backend),
                None => {
                    return Err(format!(
                        "无法识别的图形后端 `{text}`（只接受 {}）",
                        GraphicsBackend::IDS.join(" / ")
                    ))
                }
            };
        }
        Ok(next)
    }
}

/// 内存文本 → `Option<u32>`（空 = 清空；非整数/越界 = 错）。
fn parse_memory(text: &str) -> Result<Option<u32>, String> {
    let Some(text) = non_blank(Some(text)) else {
        return Ok(None);
    };
    let value: u32 = text
        .parse()
        .map_err(|_| format!("内存需要是整数（MiB），收到 `{text}`"))?;
    if !valid_memory(value) {
        return Err(format!(
            "内存超出范围（{MIN_MAX_MEMORY_MB}~{MAX_MAX_MEMORY_MB} MiB），收到 {value}"
        ));
    }
    Ok(Some(value))
}

fn non_blank(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests;
