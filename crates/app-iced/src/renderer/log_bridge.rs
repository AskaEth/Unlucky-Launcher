//! 把 `iced_wgpu` / `wgpu` 的 `log` 输出转到 stderr（判据表第 8 行的取证通道）。
//!
//! ## 为什么需要它
//!
//! `iced_wgpu` 在**真的**创建设备时会自己打一行：
//!
//! ```text
//! Selected: AdapterInfo { name: "Apple M1", backend: Metal, device_type: IntegratedGpu, … }
//! ```
//!
//! 这行只有「wgpu 走到适配器那一步」才会出现，所以它是**GPU 真生效的实证**——不是我们的推断。
//! 但它走 `log` crate，而本进程从来没装过 logger，于是 M1 那份报告里这行根本不存在：
//! 「看不到」被当成了「没有」。装一个只放行 GPU 相关 target 的 logger，成本 60 行，
//! 换来的是以后每次验收都能直接看见 wgpu 说了什么。
//!
//! ## 过滤
//!
//! 按 **target 分级**，不是一刀切：`wgpu_hal` / `wgpu_core` 在 Info 级别会刷出上百行 Vulkan
//! loader 噪声（M97 实测：启动时 90 行 registry 扫描），把真正有用的
//! `iced_wgpu: Selected: AdapterInfo{…}` 埋掉。
//!
//! | `ULL_GPU_LOG` | `iced_wgpu*` | `wgpu*`（hal / core） | 用途 |
//! |---|---|---|---|
//! | 未设 / `info` | Info | **Warn** | 默认：看得见选中的适配器/格式 + wgpu 的告警 |
//! | `warn` | Warn | Warn | 只留告警（启动日志最干净） |
//! | `debug` | Debug | Debug | 全量（诊断用；含 Vulkan loader 噪声） |
//! | `off` | — | — | 不装 logger（回到 M97 之前，仅用于对照实验） |
//!
//! 其它 target（`winit`、`iced_winit` …）一律不放行：它们只会把启动日志淹掉。

use std::sync::Once;

/// 装 logger（进程内只生效一次；重复调用无害）。
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Some(level) = level_from_env() else {
            return;
        };
        // `set_boxed_logger` 只能成功一次；已被别人装过就安静让位，不 panic。
        if log::set_boxed_logger(Box::new(Bridge { level })).is_ok() {
            log::set_max_level(level);
        }
    });
}

/// `ULL_GPU_LOG` → 过滤级别；`None` = 别装（`off`）。
fn level_from_env() -> Option<log::LevelFilter> {
    parse_level(&std::env::var("ULL_GPU_LOG").unwrap_or_default())
}

/// 取值解释（纯函数，测试直接喂字符串，不碰进程环境）。
fn parse_level(value: &str) -> Option<log::LevelFilter> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" | "0" | "no" | "false" => None,
        "warn" | "warning" => Some(log::LevelFilter::Warn),
        "debug" => Some(log::LevelFilter::Debug),
        "trace" => Some(log::LevelFilter::Trace),
        _ => Some(log::LevelFilter::Info),
    }
}

/// 某个 target 在这个总级别下实际放行到哪一级。
///
/// `wgpu_hal`/`wgpu_core` 在 `info` 档被压到 `warn`：它们的 info 是 Vulkan loader 的原文，
/// 对「到底用了哪个后端」没有信息量，只有噪声。
fn target_level(level: log::LevelFilter, target: &str) -> log::LevelFilter {
    if level == log::LevelFilter::Info && !target.starts_with("iced_wgpu") {
        return log::LevelFilter::Warn;
    }
    level
}

/// 只放行渲染后端相关 target 的 logger。
struct Bridge {
    level: log::LevelFilter,
}

impl log::Log for Bridge {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        let target = metadata.target();
        is_gpu_target(target) && metadata.level() <= target_level(self.level, target)
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // iced_wgpu 用 `{:#?}` 打多行结构体；压成一行才好 grep / 贴验收报告。
        let message = record.args().to_string().replace('\n', " ");
        // M188 复审 P2-1：`[gpu] iced_wgpu: …Selected: AdapterInfo…` 是「GPU 真生效」的**唯一**
        // 证据行，也是「为什么降级了」的第一现场，必须进 `<.ULL>/logs/launcher.log`。
        crate::app::diag::diag!("[gpu] {}: {}", record.target(), message.trim_end());
    }

    fn flush(&self) {}
}

/// `iced_wgpu::window::compositor` / `wgpu_core::…` / `wgpu_hal::…` 都算。
fn is_gpu_target(target: &str) -> bool {
    ["iced_wgpu", "wgpu"]
        .iter()
        .any(|prefix| target.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_gpu_targets_pass() {
        assert!(is_gpu_target("iced_wgpu::window::compositor"));
        assert!(is_gpu_target("wgpu_core::instance"));
        assert!(is_gpu_target("wgpu_hal::metal"));
        assert!(!is_gpu_target("iced_winit::lib"));
        assert!(!is_gpu_target("winit::event_loop"));
    }

    #[test]
    fn level_env_values_are_interpreted() {
        assert_eq!(parse_level(""), Some(log::LevelFilter::Info));
        assert_eq!(parse_level("info"), Some(log::LevelFilter::Info));
        assert_eq!(parse_level(" WARN "), Some(log::LevelFilter::Warn));
        assert_eq!(parse_level("off"), None);
        assert_eq!(parse_level("0"), None);
    }

    /// 默认档（`info`）下 `wgpu_hal`/`wgpu_core` 被压到 Warn：它们是 Vulkan loader 原文，
    /// M97 实测一屏 90 行噪声，会把 `Selected: AdapterInfo` 埋掉。
    #[test]
    fn default_level_mutes_wgpu_noise_but_keeps_adapter_line() {
        use log::LevelFilter;
        assert_eq!(
            target_level(LevelFilter::Info, "iced_wgpu::window::compositor"),
            LevelFilter::Info,
            "选中适配器那行必须留"
        );
        assert_eq!(
            target_level(LevelFilter::Info, "wgpu_hal::vulkan::instance"),
            LevelFilter::Warn,
            "Vulkan loader 噪声必须压掉"
        );
        assert_eq!(
            target_level(LevelFilter::Info, "wgpu_core::instance"),
            LevelFilter::Warn
        );
        // 显式要 debug 时全放行（诊断用）。
        assert_eq!(
            target_level(LevelFilter::Debug, "wgpu_hal::vulkan::instance"),
            LevelFilter::Debug
        );
    }
}
