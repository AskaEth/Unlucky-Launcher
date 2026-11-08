//! 字体基座（D28 前半）：把令牌的字体栈求值成「本机真的有的族名」，并打一行启动日志。
//!
//! ## 做什么
//!
//! 1. 取 `Platform::current()` 的三条栈（Display / Text / Mono，见 `unlucky_design::typography`）；
//! 2. 用 iced 的**全局字体系统**（`iced_graphics::text::font_system()`，即 cosmic-text 的
//!    `FontSystem`，首次调用会扫系统字体）核对每个族名是否装机 → 取栈里第一个装机可用的族；
//! 3. 给出 400/500/700 三档字重的落点（原生 / 最近档 / 合成加粗）：
//!    **先查系统里这个族真实提供的字面（fontdb 的 `FaceInfo.weight`）**，查不到才回落到令牌的
//!    纯函数 `Platform::resolve_weight`。缺档按降级记录。
//!
//! ## 为什么要真查系统
//!
//! 令牌栈写的是「Win11 有 Segoe UI Variable Text」，但 Win10 / Server 只有 `Segoe UI`（静态
//! 300/400/600/700，**没有 500**），干净安装的 Linux 可能一个都没有。只按栈头写死族名会把
//! 「实际使用哪个族、哪一档字重」变成猜测——本 mission 的开发机就是 Windows Server 2025，
//! 实测就回退到了经典 `Segoe UI`（见交接文档的启动日志）。

//! ## 不做什么
//!
//! **中文字体 CDN 下载不在这里**（后续 mission）。本模块只做「系统字体优先」这一半，
//! 也没有能力判断 CJK 字形覆盖（那是 `ttf-parser` 查 cmap 的活，M60 Q3 的 spike 做过）。
//!
//! ## 中文那一半（M92 / D28 后半）
//!
//! 语言探测 + 「中文环境有没有中文字体」的判据在 [`cjk`]（判据本体在零依赖的 `unlucky_i18n::font`），
//! 语言的真值源是 [`crate::platform::locale`]（Windows 上 `reg.exe` 读注册表，零 FFI），
//! 结论作为 [`Resolved::cjk`] 进同一行启动日志。**英文环境结论恒为 `NotNeeded`**：不要求中文字体、
//! 不下载任何字体（系统里已有的 CJK 族照旧躺在 iced 的字体库里，我们既不注册新的也删不掉——那是
//! cosmic-text 的全局状态，不是本模块能管的事）。

mod cjk;

use cjk::CjkReport;
use unlucky_design::typography::{FontFamily, Platform, Weight, WeightMatch};

use crate::theme::Families;

/// 一条栈的求值结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FamilyReport {
    /// 栈的用途（`text` / `display` / `mono`）。
    pub stack: &'static str,
    /// 实际选中的族名（栈里第一个装机可用的；都没装就是栈头）。
    pub family: &'static str,
    /// 选中的族是否真的在系统字体库里。
    pub installed: bool,
}

/// 一档字重的求值结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeightReport {
    /// 请求的字重（400/500/700）。
    pub requested: u16,
    /// 实际可用的字重（降级后）。
    pub available: u16,
    /// 匹配质量（原生 / 最近档 / 合成加粗）。
    pub quality: WeightMatch,
}

/// 字体求值的全部结论（启动日志与测试都用它）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// 平台（决定栈与字重覆盖表）。
    pub platform: Platform,
    /// 三条栈的落点。
    pub families: [FamilyReport; 3],
    /// 三档字重的落点（对**正文**栈求值）。
    pub weights: [WeightReport; 3],
    /// 语言 + 中文环境的中文字体策略（D28；英文环境恒为 `NotNeeded`）。
    pub cjk: CjkReport,
}

impl Resolved {
    /// 把结论塞回主题适配层要的形状。
    pub fn families(&self) -> Families {
        Families {
            text: self.families[0].family,
            display: self.families[1].family,
            mono: self.families[2].family,
        }
    }

    /// 是否有任何一档走了「合成加粗」（= 观感降级，要记日志）。
    pub fn has_synthetic(&self) -> bool {
        self.weights
            .iter()
            .any(|weight| weight.quality == WeightMatch::Synthetic)
    }

    /// 启动日志（**一行**：族 / 字重 / 语言与中文字体策略）。
    pub fn describe(&self) -> String {
        let families = self
            .families
            .iter()
            .map(|report| {
                format!(
                    "{}={:?}{}",
                    report.stack,
                    report.family,
                    if report.installed {
                        ""
                    } else {
                        "(未装机→回落)"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(" ");

        let weights = self
            .weights
            .iter()
            .map(|report| {
                let quality = match report.quality {
                    WeightMatch::Native => "native",
                    WeightMatch::Nearest => "nearest",
                    WeightMatch::Synthetic => "synthetic",
                };
                format!("w{}={}({quality})", report.requested, report.available)
            })
            .collect::<Vec<_>>()
            .join(" ");

        format!(
            "[font] platform={} {families} | weights {weights} | {}",
            self.platform.name(),
            self.cjk.describe()
        )
    }
}

/// 求值本机字体（会触发 iced 全局字体系统的首次初始化，约 0.1~0.8s）。
///
/// 中文那一级按**系统 locale** 判：启动编排（`app/boot.rs`）这一拍还没有配置读取点。真正的
/// `ui.language` 在 [`crate::app::state`] 构造 `App` 时读到，随后用 [`cjk_for`] 重算一次并打
/// 日志——所以「英文系统上手动切中文」不会停在 `NotNeeded`（见 [`cjk_for`]）。
pub fn resolve() -> Resolved {
    resolve_with(None)
}

/// Termux 上把 Android 的系统 CJK 字体注册进 cosmic-text 的字体库。
///
/// 这套 glibc 构建跑在 Termux 里（`target_os = "linux"`），fontdb 只扫 Linux 的常规字体目录，
/// Android 的 `/system/fonts` 不在其中——不注册的话中文实例名与界面文案全是方框。
/// 第一次调用加载一次；找不到任何候选字体时静默放过（由既有回退链兜底）。
fn register_termux_fonts() {
    use std::sync::OnceLock;

    use unlucky_core::platform::paths::ProcessEnv;
    use unlucky_core::platform::termux;

    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        if !termux::is_termux(&ProcessEnv) {
            return;
        }
        const CANDIDATES: &[&str] = &[
            "/system/fonts/NotoSansCJK-Regular.ttc",
            "/system/fonts/NotoSansCJKSC-Regular.otf",
            "/system/fonts/DroidSansFallback.ttf",
        ];
        let Ok(mut font_system) = iced::advanced::graphics::text::font_system().write() else {
            return;
        };
        let db = font_system.raw().db_mut();
        for path in CANDIDATES {
            if db.load_font_file(path).is_ok() {
                return;
            }
        }
    });
}

/// 同 [`resolve`]，但把配置里 `ui.language` 的显式值喂给中文策略（D28 的第一级）。
pub fn resolve_with(setting: Option<&str>) -> Resolved {
    register_termux_fonts();

    let platform = Platform::current();
    let faces = system_faces();

    let is_installed = |family: &str| -> bool {
        faces
            .as_ref()
            .is_none_or(|faces: &Vec<Face>| faces.iter().any(|face| face.family == family))
    };

    let families = [
        FamilyReport {
            stack: "text",
            family: pick(platform, FontFamily::Text, &is_installed),
            installed: is_installed(pick(platform, FontFamily::Text, &is_installed)),
        },
        FamilyReport {
            stack: "display",
            family: pick(platform, FontFamily::Display, &is_installed),
            installed: is_installed(pick(platform, FontFamily::Display, &is_installed)),
        },
        FamilyReport {
            stack: "mono",
            family: pick(platform, FontFamily::Mono, &is_installed),
            installed: is_installed(pick(platform, FontFamily::Mono, &is_installed)),
        },
    ];

    // 字重：**优先用系统里这个族真实提供的档位**（令牌表的 `WeightCoverage` 是「这个族通常有哪几档」，
    // 装机版可能是静态子集：Win Server / 精简版只有经典 Segoe UI，没有 500）。查不到才回落到令牌纯函数。
    let weights = [Weight::Regular, Weight::Medium, Weight::Bold]
        .map(|weight| weight_report(platform, families[0].family, weight, faces.as_deref()));

    // D28 后半：语言 + 中文环境的中文字体策略。`faces` 的 `None` 在这里是「字体库读不出来」，
    // 适配层会把它翻成 `Unprobeable` 而不是「缺字体」（否则会误触发一次 CDN 下载）。
    let cjk_report = cjk::probe_with(setting, faces.as_deref());

    Resolved {
        platform,
        families,
        weights,
        cjk: cjk_report,
    }
}

/// 语言切换后的中文策略重算（M106）：`setting` 是配置里 `ui.language` 的**显式值**
/// （`None` = 跟随系统，由 [`crate::platform::locale::SystemLocale`] 决定）。
///
/// 为什么需要它：启动那一轮（[`resolve`]）拿不到配置，只按系统 locale 判。用户在英文系统上
/// 手选「简体中文」时，若不复算，策略会停在 `NotNeeded`——界面文字是中文、字体却没有中文字形，
/// 那就是白块。复算出的 `NeedsDownload` 是 D28 的**信号**（下载器仍是 `Source::PendingPin`，
/// 见 `unlucky_i18n::font`），本 crate 只负责把它打进日志、不让它静默。
pub fn cjk_for(setting: Option<&str>) -> CjkReport {
    cjk::probe_with(setting, system_faces().as_deref())
}

/// 一条系统字面（族名 + 字重）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Face {
    family: String,
    weight: u16,
}

/// 某档字重的落点：先问系统，再问令牌。
fn weight_report(
    platform: Platform,
    family: &str,
    weight: Weight,
    faces: Option<&[Face]>,
) -> WeightReport {
    let requested = weight.value();

    let available = faces.and_then(|faces| {
        let mut weights = faces
            .iter()
            .filter(|face| face.family == family)
            .map(|face| face.weight)
            .collect::<Vec<_>>();
        weights.sort_unstable();
        weights.dedup();
        if weights.is_empty() {
            return None;
        }

        Some(
            weights
                .iter()
                .copied()
                .min_by_key(|candidate| candidate.abs_diff(requested))
                .unwrap_or(requested),
        )
    });

    match available {
        Some(available) => WeightReport {
            requested,
            available,
            quality: quality_of(requested, available),
        },
        None => {
            let choice = platform.resolve_weight(FontFamily::Text, weight);
            WeightReport {
                requested: choice.requested,
                available: choice.available,
                quality: choice.quality,
            }
        }
    }
}

/// 与令牌层同一判据：原生 / 最近档（差 <200）/ 合成加粗（差 ≥200）。
fn quality_of(requested: u16, available: u16) -> WeightMatch {
    match available.abs_diff(requested) {
        0 => WeightMatch::Native,
        delta if delta >= 200 => WeightMatch::Synthetic,
        _ => WeightMatch::Nearest,
    }
}

/// 在栈里选第一个**装机可用**的族（跳过 `sans-serif` 这类通用族名）。
///
/// 一个都没装上时返回栈头：让 cosmic-text 自己去回退，比我们猜一个更靠谱。
pub fn pick(
    platform: Platform,
    stack: FontFamily,
    is_installed: &dyn Fn(&str) -> bool,
) -> &'static str {
    let faces = platform.font_stack(stack);

    faces
        .iter()
        .find(|face| !is_generic(face.family) && is_installed(face.family))
        .map_or(faces[0].family, |face| face.family)
}

/// 通用族名（CSS 关键字，不是真族名）。
fn is_generic(family: &str) -> bool {
    matches!(family, "sans-serif" | "serif" | "monospace" | "cursive")
}

/// 系统字体库里的全部字面（`None` = 拿不到，视为「不知道」，全部当装机）。
fn system_faces() -> Option<Vec<Face>> {
    let lock = iced::advanced::graphics::text::font_system();
    let mut font_system = lock.write().ok()?;

    let mut faces = Vec::new();
    for face in font_system.raw().db().faces() {
        for (family, _) in &face.families {
            faces.push(Face {
                family: family.clone(),
                weight: face.weight.0,
            });
        }
    }

    Some(faces)
}

#[cfg(test)]
mod tests;
