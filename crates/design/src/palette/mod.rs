//! # 配色来源（D38 / D40）：图片取色 · 内置色卡 · 三级回落 · WCAG 收敛
//!
//! 用户在 2026-11-01 定的配色来源是三级（D38）：
//!
//! ```text
//! ① 启动器背景图取色（用户设了背景图时）
//!    ↓ 没有背景图 / 取色失败
//! ② 桌面壁纸取色（用户开了「莫奈取色」但没设背景图时）
//!    ↓ 没有壁纸 / 取色失败 / 对比度收敛失败
//! ③ 内置色卡（用户手动选的那张；没选过就是出厂默认「天蓝」）
//! ```
//!
//! 取色失败**不静默**：本模块的 [`Provenance`] 会把「最终用了哪一级」和「前几级为什么没用」
//! 都带回来，设置页据此显示提示；解码失败也在里面（它不在这里发生，但原因是这里记录的）。
//! 契约（UI 怎么调、文案怎么映射）见 `DevDocs/SubAgentsTalk/palette-source-contract.md`。
//!
//! ## 分层（D40）：本模块只做纯数学
//!
//! | 层 | 在哪 | 职责 | 依赖 |
//! |---|---|---|---|
//! | 纯数学 | **本模块**（`crates/design`） | 像素 → 种子 → 49 角色；色卡；回落决策；对比度收敛 | 无（零第三方依赖） |
//! | 解码 | `crates/palette-extract` | 路径/字节 → RGBA 像素 | `image`（唯一引入处） |
//! | 调用侧 | 后续 appearance/settings mission | 拿路径、把 [`Provenance`] 变文案 | UI 框架 |
//!
//! 所以本模块的「图片输入」永远是**已经解码好的像素**，从来没有路径、没有 IO。
//!
//! ## 四个部分
//!
//! - [`seed`]：`seed_from_rgba(pixels, w, h)`（D40 的入口形状）。量化在 [`quantize`]，
//!   挑主色在 [`score`]——**不是取平均色**，理由见 [`quantize`] 的模块文档。
//! - [`card`]：内置色卡（≥6 张，含出厂默认「天蓝」）。卡的 `key` 是**持久化键**，写进用户设置。
//! - [`wcag`]：对任意一套方案跑对比度断言；不达标就沿 tone 轴修正，并记录「改了什么」。
//! - [`visibility`]：容器层可视性裁决（状态容器对 `surface` 的 1.20:1 门禁 + 容器阶梯单调），
//!   即债务台账「SecondaryContainer 对 Surface 仅 1.06」的令牌层结论。
//! - [`resolve`]：纯函数 `resolve_source(...) -> (Scheme, Provenance)`，实现三级优先与回落。
//!
//! ## D40 的两条实测事实（不是风格偏好，是物理事实）
//!
//! 1. **零彩度种子的 CAM16 色相是常数 209.49°**，不是 NaN。所以「灰白壁纸」硬取色会得到
//!    一个凭空上色的青色（`#006874`），且极低彩度段色相是噪声放大器（`#808080` → 209.5°、
//!    `#7A7C7E` → 231.2°）。→ [`seed::MIN_SEED_CHROMA`] = 8 的种子彩度门槛，低于它就回落色卡。
//! 2. **动态色也必须过 WCAG**。候选色卡的 208 项门禁（`tests/contrast_aa.rs`）不覆盖取色结果，
//!    所以 [`wcag`] 对**任何**来源的方案都跑同一套检查，不达标就收敛或回落。

pub mod card;
pub mod resolve;
pub mod seed;
pub mod visibility;
pub mod wcag;

mod quantize;
mod score;

pub use card::{Card, CardKey, CARDS, DEFAULT_CARD};
pub use resolve::{resolve_source, ImageInput, ImageSource, Level, Outcome, Provenance, Skip};
pub use seed::{
    analyze_rgba, seed_from_rgba, Failure, Seed, MAX_SAMPLES, MIN_PIXELS, MIN_SEED_CHROMA,
    MUTED_SEED_CHROMA,
};
pub use visibility::{
    check_surface_hierarchy, LayerStep, SurfaceHierarchy, LAYER_ROLES, LAYER_STEP_MIN_LSTAR,
    STATE_CONTAINERS, STATE_CONTAINER_MIN,
};
pub use wcag::{converge, Adjustment, WcagReport, AA_NON_TEXT, AA_TEXT, TEXT_PAIRS};

/// 一套方案的来源层级（[`Level`] 的别名，读起来更清楚）。
pub use resolve::Level as SourceLevel;
