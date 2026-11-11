//! 平台相关的窗口能力（P4 / D36 / M116 / M168 / M182 / M196）：整窗透明度与窗口圆角的**判据**、
//! 两处 `unsafe` 落点、系统语言真值源、以及数据根派生——每一件各归一处（判据不碰 FFI，
//! FFI 只住落点文件）。
//!
//! - 本文件 = 整窗 α 的纯判据 [`effective_window_alpha`]：不读环境变量、不调 FFI、不碰窗口句柄，
//!   只回答「按这个渲染后端和用户要的 α，最终该给窗口什么值」；
//! - [`win32::corner`] = 圆角的纯判据 [`plan_corner`] / [`plan_corner_for`]（M116 / M182）：
//!   用户策略 × 宿主平台 × 渲染档 → 来源与原因码。两个判据都**不碰** FFI，因此都能被断言证明；
//! - [`win32::transparency`] = 整窗 α 的 FFI 落点（补 `WS_EX_LAYERED` +
//!   `SetLayeredWindowAttributes`，以及 M116 的 `DwmSetWindowAttribute`）；
//! - [`win32::console`] = M196 的控制台附着落点（`AttachConsole`）：GUI 子系统进程的输出
//!   为什么在 cmd 里会静默丢失，以及唯一的补救。两个落点的放行范围各自收在一个文件里，
//!   `unsafe-scope` 门禁的期望集合就是这两个路径（集合相等，不是黑名单）；
//! - [`locale`] = 系统语言真值源（D28）：`reg.exe` 子进程读注册表，**零 `unsafe` / 零 FFI**。
//!
//! 为什么判据要单独拎出来：D36 的降级规则（α 下限 0.08 / `≤0` 一律拒绝 / CPU 档也允许
//! 窗口级 α）**可以**被断言证明；而 FFI 的**视觉**结果在本仓库的验证环境（RDP + 虚拟显示
//! 适配器、无 Mac 真机）上没有参考价值。把可测的那一半独立出来，才有「有测试的判据 +
//! 没测试的观感」这条诚实的分界。
//!
//! ## 平台分册（M182：非 Windows 不再是「未知」一格）
//!
//! | 模块 | 里面有什么 | 有没有 FFI |
//! |---|---|---|
//! | [`data`] | 数据根派生（字体缓存分区 + 一条设置键的生效值，M168） | 没有 |
//! | [`host`] | [`HostOs`]：平台族的**值**（判据的输入，不是散落的 `cfg`） | 没有 |
//! | [`macos`] | macOS 为什么是直角的三环证据 + 「迁移原生标题栏」的未实现配方 | 没有（本轮只有文档） |
//! | [`linux`] | X11 / Wayland 谁负责圆角、为什么我们不自绘 | 没有（本轮只有文档） |
//! | [`win32`] | Windows 的实现与探针（DWM / 分层 / 控制台附着） | **有**（两处落点：`transparency` + `console`） |
//!
//! 两个新平台模块**只有文档、没有代码**：M182 的裁决是不实现（macOS 要动手就得先迁移原生标题栏，
//! 那会引入两套窗口控件，必须连着撤掉自绘三键 + 真机验收一起做）。把结论写在这里而不是写进报告，
//! 是因为报告会随会话消失，而下一轮动这块的人一定先读代码。

use unlucky_presenter::prefs::WINDOW_ALPHA_MIN;

use crate::Backend;

pub mod data;
pub mod host;
pub mod linux;
pub mod locale;
pub mod macos;
pub mod win32;

/// 窗口圆角的**判定**（M116 / M182）住在 `win32::corner`：判据是用户策略 × 宿主平台 × 渲染档。
/// 非 Windows 的两档（macOS = 合成器不裁 borderless 窗、Linux = CSD 没人裁）在
/// [`macos`] / [`linux`] 两个模块里逐条给了证据。
pub use host::HostOs;
pub use win32::corner::{plan_corner, plan_corner_for, CornerExclusion, CornerPlan, CornerSource};

/// 本机 Windows 版本（`None` = 非 Windows / 读不到）。**进程内只探测一次**。
///
/// 为什么缓存：底层是 `reg.exe` 子进程（`unlucky_core::platform::windows_version`，实测约 50 ms），
/// 而圆角判定会在启动、切档、热重载时各问一次。探测结果在进程生命周期内不会变。
///
/// 为什么走 `unlucky_presenter::prefs` 的重导出而不是 `unlucky_core` 直连：
/// `unlucky-app-iced` 不直接依赖 core（分层：UI → presenter → core），这是既有的唯一通道。
#[must_use]
pub fn current_windows_version() -> Option<unlucky_presenter::prefs::WindowsVersion> {
    static CACHED: std::sync::OnceLock<Option<unlucky_presenter::prefs::WindowsVersion>> =
        std::sync::OnceLock::new();
    *CACHED.get_or_init(unlucky_presenter::prefs::probe_windows_version)
}

/// 用户请求的整窗 α → 真正写进窗口的值（纯函数）。
///
/// - `requested <= 0.0`（含 `-0.0`）→ `None`：**拒绝整窗穿透**。α = 0 表示窗口内容全透明，
///   用户会以为程序消失/卡死，所以这不是「合法的最小值」而是「不给设」（D36）。
///   非有限值（`NaN` / `±∞`）同样 `None`——它们没有合法落点，也不该被 `clamp` 悄悄吞掉。
/// - 其余一律夹到 [`WINDOW_ALPHA_MIN`]`..=1.0`。
/// - 后端**不**参与「能不能用窗口级 α」的判断：D36 **修订**了 D27 的旧结论——
///   CPU 渲染档同样允许整窗 α（CPU 档被限制的是面板 α / 玻璃 / 模糊，不是窗口 α）。
///   因此三个后端分支给出同一个值；写成 `match` 是为了让「将来真分档」只改这一处，
///   而不是让读代码的人以为这里漏了一个分支。
#[must_use]
pub fn effective_window_alpha(backend: Backend, requested: f64) -> Option<f64> {
    if !requested.is_finite() || requested <= 0.0 {
        return None;
    }
    match backend {
        Backend::Gpu | Backend::Cpu | Backend::Auto => Some(requested.clamp(WINDOW_ALPHA_MIN, 1.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::effective_window_alpha;
    use crate::Backend;
    use unlucky_presenter::prefs::WINDOW_ALPHA_MIN;

    const BACKENDS: [Backend; 3] = [Backend::Gpu, Backend::Cpu, Backend::Auto];

    #[test]
    fn truth_table_valid_alpha_passes_through_every_backend() {
        // GPU / CPU × 合法值：四个格子都要 Some（CPU 档不再强制不透明）。
        for backend in BACKENDS {
            assert_eq!(
                effective_window_alpha(backend, 0.5),
                Some(0.5),
                "{backend:?}：合法 α 原样返回"
            );
            assert_eq!(effective_window_alpha(backend, 1.0), Some(1.0));
            assert_eq!(
                effective_window_alpha(backend, WINDOW_ALPHA_MIN),
                Some(WINDOW_ALPHA_MIN)
            );
        }
    }

    #[test]
    fn truth_table_out_of_range_is_clamped_not_refused() {
        for backend in BACKENDS {
            assert_eq!(
                effective_window_alpha(backend, 0.001),
                Some(WINDOW_ALPHA_MIN),
                "{backend:?}：正数低于下限 → 夹到 0.08（钳位，不拒绝）"
            );
            assert_eq!(
                effective_window_alpha(backend, 9.0),
                Some(1.0),
                "{backend:?}：高于 1.0 → 夹到 1.0"
            );
        }
    }

    #[test]
    fn truth_table_non_positive_is_refused_on_every_backend() {
        for backend in BACKENDS {
            for requested in [0.0, -0.0, -0.01, -1.0] {
                assert_eq!(
                    effective_window_alpha(backend, requested),
                    None,
                    "{backend:?}：requested={requested} → 拒绝整窗穿透"
                );
            }
        }
    }

    #[test]
    fn truth_table_non_finite_is_refused() {
        // NaN 用 `<= 0.0` 判不出来（比较恒 false），所以必须先 `is_finite()`；
        // 否则 NaN 会穿到 `clamp` 里再原样返回，变成「给窗口一个 NaN」。
        for backend in BACKENDS {
            for requested in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                assert_eq!(
                    effective_window_alpha(backend, requested),
                    None,
                    "{backend:?}：requested={requested} 不是有限值"
                );
            }
        }
    }

    #[test]
    fn cpu_tier_is_not_forced_opaque() {
        // D36 对 D27 的**修订**：旧文档写「CPU 档一律不透明」，这条测试钉住修订后的行为，
        // 防止谁照着旧文档把 CPU 分支改回 `None`。
        assert_eq!(effective_window_alpha(Backend::Cpu, 0.5), Some(0.5));
        assert_eq!(effective_window_alpha(Backend::Gpu, 0.5), Some(0.5));
    }

    /// `appearance.palette_source` 的码值必须与 `design` 的 `Level::code()` 逐字一致。
    ///
    /// 为什么这条断言住在这里：`unlucky-core` 按分层纪律不依赖 `unlucky-design`，
    /// 它的测试**看不到** design，所以 core 自己只能钉字面量。本 crate 同时依赖
    /// `unlucky-design`（`Level`）与 `unlucky-presenter`（重导出 `PaletteSource`），
    /// 是全仓唯一能真做这次对照的地方。
    ///
    /// 抓得住：design 侧改任一档的稳定码（例如把 `wallpaper` 改成 `desktop`）、或有人改了
    /// core 的字符串表——两个方向都红（下面往返那条覆盖「只改一侧」）。
    /// 抓不住：档位的**语义**是否仍对齐（只保证字符串一致），以及 UI 是否真按这个码值取色。
    #[test]
    fn palette_source_codes_match_design_level_codes() {
        use unlucky_design::palette::Level;
        use unlucky_presenter::prefs::PaletteSource;

        for (source, level) in [
            (PaletteSource::BackgroundImage, Level::BackgroundImage),
            (PaletteSource::Wallpaper, Level::Wallpaper),
            (PaletteSource::Preset, Level::Preset),
        ] {
            assert_eq!(
                source.as_str(),
                level.code(),
                "core 的 {source:?} 码值与 design 的 {level:?} 不一致"
            );
            // 反方向：design 的码值喂回 core 必须还原成同一档（防「两个字符串互换了」）。
            assert_eq!(PaletteSource::from_key_value(level.code()), source);
            // 且必须真的在 core 的允许值表里（否则 schema 会拒绝写这个值）。
            assert!(unlucky_presenter::prefs::PALETTE_SOURCES.contains(&level.code()));
        }
    }
}
