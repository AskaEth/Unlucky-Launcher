//! 实例级启动参数的**落位**：`ResolvedLaunch` → [`LaunchRequest`]。
//!
//! 三级回落本身在 [`crate::game::launch_overrides`]（纯计算）；本模块只回答「落到哪一段」，
//! 规则与理由见 M157 的定案（叠加/冲突），这里给出可执行的对照表：
//!
//! | 生效值 | 落点 | 组装表 | 为什么在这儿 |
//! |---|---|---|---|
//! | 全局 `java.extra_args` | `user.jvm` | 第 7 项 | 基线：按“默认值”处理，可被清单参数抑制 |
//! | 实例 `launch.jvmArgs` | `user.override_jvm` | 第 4 项 | 精调：最高优先级，`-Xmx`/编码/代理默认值全被它抑制 |
//! | 实例 `launch.gameArgs` | `user.game` | 第 32 项 | 最后追加，可覆盖前面一切游戏参数 |
//! | `memoryMb`（已回落） | `memory.max_mb` | 第 5 项 | `-Xmx` 只从这里长出来 |
//! | `graphicsBackend` | `graphics_backend` | 第 31 项 | 仍过 `graphics_backend_allowed` 门槛，不放行给诊断告警 |
//! | `javaPath` | **不进来** | — | Java 选择在调用方（`JavaSelection`），见标量注释 |
//!
//! # 两条必须一起读的结论
//!
//! 1. **追加，不替换**：全局与实例的 JVM 参数都下发。实例段在前、全局段在后，同名选项由 JVM
//!    取后出现者——但注意组装表第 4 项在 `-Xmx` **之前**：实例文本里的 `-Xmx` 会抑制内存字段
//!    产出的 `-Xmx`（这是「文本比字段更精确」的表达，不是 bug）；
//! 2. **全局文本里的 `-Xmx` 不生效**：第 7 项晚于第 5 项，`-Xmx` 已被内存字段占住
//!    （`add_default` 语义）。要改堆请用内存字段或实例 JVM 文本——写进注释与测试，别当特例。
//!
//! `javaPath` 不进 `LaunchRequest`：那份结构只收选好的 [`crate::launch::request::JavaSelection`]，
//! 调用方用 `resolved.java_path` 决定「显式路径优先 / 自动挑」。

use std::path::Path;

use crate::game::launch_overrides::ResolvedLaunch;
use crate::launch::command_builder::tokenize;
use crate::launch::request::{LaunchRequest, MemoryOptions};

/// 把三级回落后的生效值写进请求（覆盖 `user.*` / `memory` / `graphics_backend` 三处）。
///
/// 调用方在 [`crate::launch::adapter::build_request`] 之后调用一次即可；它**接管**这三处，
/// 所以别在此之前再往 `user` 里塞东西（目前 CLI 与 GUI 都没有别的来源，契约就是这一段）。
pub fn apply(request: &mut LaunchRequest, resolved: &ResolvedLaunch) {
    request.user.jvm = tokens(resolved.global_jvm_args.as_deref());
    request.user.override_jvm = tokens(resolved.instance_jvm_args.as_deref());
    request.user.game = tokens(resolved.game_args.as_deref());
    request.memory = MemoryOptions {
        min_mb: request.memory.min_mb,
        max_mb: Some(resolved.memory_mb),
    };
    request.graphics_backend = resolved.graphics_backend.flag_value().map(str::to_string);
}

/// 实例/全局给的显式 Java 路径（`None` = 调用方走自动挑）。
#[must_use]
pub fn java_binary(resolved: &ResolvedLaunch) -> Option<&Path> {
    resolved.java_path.as_deref()
}

/// 版本能不能下发 `--graphicsBackend`：**解析过继承链的**版本 JSON + 游戏版本号。
///
/// `resolved_json` 必须是继承解析之后的那份（26.2 的 `lwjgl-vulkan` 在父层，
/// 只看 loader 层会漏）。判据本身仍是 [`graphics_backend_allowed`] 一处实现——
/// UI 与拼装链共用它，别在调用方再抄一遍能力位判断。
#[must_use]
pub fn backend_allowed(
    version: &crate::launch::game_version::GameVersion,
    resolved_json: &crate::game::VersionJson,
) -> bool {
    crate::launch::game_args::graphics_backend_allowed(
        version,
        crate::game::VersionCapabilities::from_json(resolved_json).has_vulkan_backend,
    )
}

fn tokens(text: Option<&str>) -> Vec<String> {
    text.map(tokenize).unwrap_or_default()
}

#[cfg(test)]
mod tests;
