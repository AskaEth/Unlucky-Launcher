//! 本进程**实际**用的是哪个渲染后端（D36 的「CPU 档」判据）。
//!
//! ## 为什么不能直接看 `--renderer` 的请求值
//!
//! `Options::backend` 是**请求**（默认 `Auto`）。真正落到哪个渲染器由 `probe::apply` 决定，
//! 它把结论写进进程环境变量 `ICED_BACKEND`（iced 只认这个）。于是默认参数 + 没有独显的机器上：
//! 请求是 `Auto`，实际是 tiny-skia（CPU）——如果面板 α / 玻璃的降级判据只看请求值，
//! D36 明写的「CPU 档面板 α 强制 1.0」**恰好在它最该生效的场景失效**（复审 r1 P2-2）。
//!
//! 所以判据统一走 [`effective_backend`]：它把 `ICED_BACKEND` 翻译回 [`Backend`]。
//! 这里是**唯一**的翻译点，别在别处再读一遍环境变量。
//!
//! ## M97 之后这条链是准的（以前不是）
//!
//! 以前 `ICED_BACKEND` 可能是**候选列表**（`wgpu,tiny-skia`），运行期 wgpu 静默回退到
//! tiny-skia 时这个变量还停在 wgpu ⇒ 本模块按 **GPU 档**判 ⇒ **半透明在 CPU 渲染器上照开**。
//! 也就是说「静默降级」不只让日志说谎，还让 D36 的降级判据一起说谎。
//!
//! 现在 `ICED_BACKEND` **永远是单项**（[`crate::renderer::Kind`]），而且 `app::boot` 的重试
//! 在降级重开前会把它改成 `tiny-skia`，于是：**真降级到 CPU 的那一次，面板 α 会正确地强制 1.0**。
//!
//! ## 已知边界（诚实登记）
//!
//! `iced` 0.14 没有公开的「当前后端」查询，所以本模块读的仍是**我们请求的那一项**。
//! 单项 + 重试（[`crate::renderer`] 判据表第 8 行）把「请求 ≠ 实际」的窗口压到只剩一种：
//! 重试那次自己也失败——那时候进程会带着具名错误退出，不会带着错的档位继续画。

use crate::renderer::Kind;
use crate::Backend;

/// `ICED_BACKEND` 的取值 → 后端（纯函数，好测）。
///
/// 逗号列表按**首项**解释：兼容 M97 之前写下的值（以及外部手设的实验值），
/// 但本 crate 不再产出那种值（守门测试见 `probe::tests::apply_writes_exactly_one_backend`）。
#[must_use]
pub(super) fn parse_kind(env: &str) -> Option<Kind> {
    match env.trim().split(',').next()?.trim() {
        "tiny-skia" => Some(Kind::TinySkia),
        "wgpu" => Some(Kind::Wgpu),
        _ => None,
    }
}

/// `ICED_BACKEND` → 实际档位（纯函数，好测）。
///
/// | `ICED_BACKEND` | 含义（`probe::apply` 的产物） | 结论 |
/// |---|---|---|
/// | `tiny-skia` | 只有 CPU（`--renderer=cpu` / 未编 wgpu / 探测无硬件 / wgpu 失败后重开） | [`Backend::Cpu`] |
/// | `wgpu` | 判据表第 3、6 行：起步就是 GPU（起不来会打印具名错误并重开成 `tiny-skia`） | [`Backend::Gpu`] |
/// | 未设 / 别的值 | 不是本 crate 写的（外部手设 / M97 之前的旧进程） | 请求值（不猜） |
#[must_use]
pub(super) fn backend_from_env(request: Backend, env: Option<&str>) -> Backend {
    env.and_then(parse_kind).map_or(request, Kind::tier)
}

/// 本进程实际生效的后端（判据 = `probe::apply` 写下的 `ICED_BACKEND`）。
#[must_use]
pub(super) fn effective_backend(request: Backend) -> Backend {
    backend_from_env(request, std::env::var("ICED_BACKEND").ok().as_deref())
}

/// 本进程实际生效的**后端名**（`None` = 变量没写 / 不认识）。
///
/// 与 [`effective_backend`] 的区别：那个给「档位」，这个给「名字」，用于**取证日志**
/// （`[shot] … renderer=wgpu`）。两张图分辨不出 GPU/CPU 的时候，靠的就是这一行。
#[must_use]
pub(super) fn effective_kind() -> Option<Kind> {
    std::env::var("ICED_BACKEND")
        .ok()
        .as_deref()
        .and_then(parse_kind)
}

#[cfg(test)]
mod tests {
    use super::backend_from_env;
    use crate::Backend;
    use unlucky_presenter::prefs::{PANEL_ALPHA_MIN, WINDOW_ALPHA_MIN};

    /// 抓得住：`probe::apply` 现在产出的**每一种单项值**都被翻译成正确的档位，
    /// 且 M97 之前写下的逗号列表仍按首项解释（向后兼容）。
    /// 抓不住：运行期重试降级那一刻的时序（那由 `app::boot` 的重试保证，见 `renderer` 判据表第 8 行）。
    #[test]
    fn every_iced_backend_value_maps_to_a_tier() {
        // 本 crate 现在只产单项（`probe::tests::apply_writes_exactly_one_backend` 守这一条）。
        assert_eq!(
            backend_from_env(Backend::Auto, Some("tiny-skia")),
            Backend::Cpu,
            "M97 判据表第 5 行：探测无硬件 ⇒ CPU 档"
        );
        assert_eq!(
            backend_from_env(Backend::Auto, Some("wgpu")),
            Backend::Gpu,
            "M97 判据表第 6 行：有硬件 ⇒ GPU 档"
        );
        assert_eq!(
            backend_from_env(Backend::Cpu, Some("tiny-skia")),
            Backend::Cpu,
            "强制 CPU 也是 CPU 档"
        );
        // 兼容 M97 之前的逗号列表（外部手设 / 旧进程留下）：按首项解释。
        assert_eq!(
            backend_from_env(Backend::Auto, Some("tiny-skia,wgpu")),
            Backend::Cpu
        );
        assert_eq!(
            backend_from_env(Backend::Auto, Some("wgpu,tiny-skia")),
            Backend::Gpu
        );
        assert_eq!(
            backend_from_env(Backend::Gpu, Some("wgpu,tiny-skia")),
            Backend::Gpu
        );
        // 未设 / 空白：保持请求值（不是本 crate 写的，不猜）。
        assert_eq!(backend_from_env(Backend::Auto, None), Backend::Auto);
        assert_eq!(backend_from_env(Backend::Auto, Some("  ")), Backend::Auto);
        // 不认识的取值不猜：保留请求值。
        assert_eq!(
            backend_from_env(Backend::Auto, Some("metal")),
            Backend::Auto
        );
    }

    /// **复审 r1 P2-2 的守门**：默认档（`Auto`）在无独显机器上必须落进 CPU 档，
    /// 从而让「面板 α / 玻璃强制 1.0」在默认路径上真的生效。
    ///
    /// 抓得住：判据走 `effective_backend()` 这条链（谁把 `State::new` 改回只吃请求值，这条会红）。
    /// 抓不住：整窗 α 在 CPU 档仍要生效（那是另一条判据，见 `alpha` 模块真值表）。
    #[test]
    fn auto_with_a_software_renderer_forces_the_panel_opaque() {
        use crate::pages::appearance::alpha::{effective_glass, effective_panel_alpha};

        let backend = backend_from_env(Backend::Auto, Some("tiny-skia"));
        assert_eq!(backend, Backend::Cpu);
        assert_eq!(
            effective_panel_alpha(backend, 0.55),
            1.0,
            "D36：CPU 档面板 1.0"
        );
        assert!(!effective_glass(backend, true), "D36：CPU 档玻璃强制关");
        // 但整窗 α 不受限（D36 对 D27 的修订）。
        assert!(crate::platform::effective_window_alpha(backend, 0.8).is_some());

        // 反方向：GPU 档不做这个降级（否则「强制」就没意义了）。
        let gpu = backend_from_env(Backend::Auto, Some("wgpu"));
        assert_eq!(effective_panel_alpha(gpu, 0.55), 0.55);
        assert!((PANEL_ALPHA_MIN - 0.4).abs() < 1e-9 && (WINDOW_ALPHA_MIN - 0.08).abs() < 1e-9);
    }

    /// **M97 的新守门**：wgpu 失败重开成 `tiny-skia` 之后，档位必须跟着变成 CPU 档。
    ///
    /// 这是修之前做不到的一条：那时变量停在 `wgpu,tiny-skia`，静默回退到 tiny-skia 之后
    /// 判据仍然读 `Gpu` ⇒ 半透明在 CPU 渲染器上照开。现在 `app::boot` 重开前会改写这个值，
    /// 这条测试钉的是「改写成 tiny-skia 之后判据确实翻转」。
    /// **两个后端取值各自对应哪个档位、面板 α 各是多少。**
    ///
    /// ⚠️ 这条**不**守「降级后档位翻转」——它只是把两个字面量喂给纯函数，`respawn` 一行都没执行。
    /// 复审 M97 r1 p2-1 就是靠这一点把 `respawn` 里的 `.env("ICED_BACKEND", …)` 删掉而测试全绿的。
    /// 真正的翻转由 `app::boot::tests::fallback_env_carries_the_backend_and_the_gate`
    /// （respawn 到底传了什么）钉住——那条红了才是「翻转坏了」。
    #[test]
    fn each_backend_value_maps_to_its_tier_and_alpha() {
        use crate::pages::appearance::alpha::effective_panel_alpha;

        let gpu = backend_from_env(Backend::Auto, Some("wgpu"));
        assert_eq!(gpu, Backend::Gpu);
        assert_eq!(effective_panel_alpha(gpu, 0.55), 0.55);

        let cpu = backend_from_env(Backend::Auto, Some("tiny-skia"));
        assert_eq!(cpu, Backend::Cpu);
        assert_eq!(
            effective_panel_alpha(cpu, 0.55),
            1.0,
            "D36：CPU 档面板 α 强制 1.0"
        );
    }
}
