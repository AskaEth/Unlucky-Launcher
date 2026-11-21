//! 渲染档（D27）在 [`Ui`] 上的**载体**：字段读写 + 真值翻译 + 取证日志的唯一措辞点（M442）。
//!
//! ## 这一层只是「载体」，不是观感
//!
//! 本模块只回答一个问题：**这个 `Ui` 认为自己在 GPU 档还是 CPU 档**。它不取色、不改 α、不碰玻璃——
//! 消费者在后面才读它（`GlassSpec::for_tier(tier, render_tier)`，`crates/design/src/glass.rs`）。
//! 所以落地这一刀时观感必须逐像素不变：两张改前/改后截图 0 差异就是这条的验收证据。
//!
//! ## 默认 = GPU，以及这个默认值的代价
//!
//! 构造 `Ui` 时给 [`RenderTier::Gpu`]（与 `GlassSpec::new` 的缺省一致）。那**不是**在断言「这台机器有
//! 显卡」，而是在说「没人告诉我，按产品默认路径算」。代价很实在：**任何一处漏接，进程就会以为自己是
//! GPU 档**，而 D27 对 CPU 档的要求是「禁模糊 / 禁半透明」——默认错了，CPU 档用户会拿到玻璃。
//! 所以真值必须接进来，且**三处必须同改**（漏一处就退回默认）：
//!
//! | # | 位置 | 拿什么 |
//! |---|---|---|
//! | ① | `app/state/theme.rs` 的 `rebuild_ui` | `app::backend::effective_kind()`（`ICED_BACKEND`，`probe::apply` 在开窗前写好） |
//! | ② | 同文件的 `sync_theme_fade` | 同上；**主题淡入每帧都重建 `Ui`**，漏了档位会在淡入期间静默退回默认 |
//! | ③ | `pages/harness/app.rs` 的启动闭包 | 闭包参数 `renderer::Kind` 就是这次出图用的实际后端 |
//!
//! ## `ICED_BACKEND` 未设时映射成什么（最容易埋雷的一格）
//!
//! [`of_kind`] 里 `None` → **GPU**，与字段默认值同一个口径。理由：`app::backend` 的规矩是「未设 =
//! 不是本 crate 写的，不猜」，而那种进程（单测、出图夹具、外部手设）本来就没有真后端可言——给产品
//! 默认档，最不容易制造「测试里是 CPU、产品里是 GPU」的假象。产品路径上它一定已被 `probe::apply`
//! 写过（判据表第 1~7 行的每一行都写单项），所以这一格只服务测试与夹具。

use unlucky_design::glass::RenderTier;

use crate::renderer::Kind;

use super::Ui;

impl Ui {
    /// 这个 `Ui` 认为自己在哪个渲染档（D27）。默认 [`RenderTier::Gpu`]，代价见模块文档。
    pub fn render_tier(&self) -> RenderTier {
        self.render_tier
    }

    /// 换渲染档：**只写这一个字段**，不动颜色 / 面板 α / 排版 / 主题名（默认 `Gpu`）。
    #[must_use]
    pub fn with_render_tier(mut self, tier: RenderTier) -> Self {
        self.render_tier = tier;
        self
    }
}

/// `ICED_BACKEND` 翻译出来的后端 → 渲染档。`None`（未设 / 不认识）→ [`RenderTier::Gpu`]（见模块文档）。
pub fn of_kind(kind: Option<Kind>) -> RenderTier {
    match kind {
        // 唯一的 CPU 判据：这个进程真的跑在 tiny-skia 上。
        Some(Kind::TinySkia) => RenderTier::Cpu,
        // `wgpu` = 真 GPU；`None` = 本 crate 没判过后端（单测 / 夹具 / 外部手设）→ 产品默认档。
        Some(Kind::Wgpu) | None => RenderTier::Gpu,
    }
}

/// 渲染档的稳定名字（日志用）。
pub fn name(tier: RenderTier) -> &'static str {
    match tier {
        RenderTier::Gpu => "gpu",
        RenderTier::Cpu => "cpu",
    }
}

/// 档位的**来源**（日志用）：把「凭什么这么判」跟档位写在同一行，读日志的人不必反推。
pub fn source(kind: Option<Kind>) -> &'static str {
    match kind {
        Some(Kind::Wgpu) => "ICED_BACKEND=wgpu",
        Some(Kind::TinySkia) => "ICED_BACKEND=tiny-skia",
        None => "ICED_BACKEND 未设",
    }
}

/// 一行取证（产品路径的 `[render]` 行）：`tier=cpu source=ICED_BACKEND=tiny-skia`。
pub fn line(kind: Option<Kind>) -> String {
    format!("tier={} source={}", name(of_kind(kind)), source(kind))
}
