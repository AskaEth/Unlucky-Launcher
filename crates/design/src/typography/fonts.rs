//! 三平台字体族回退栈（Win10+ / macOS 12+ / Linux）+ **字重覆盖**模型。
//!
//! 只写族名，不写文件路径、不内嵌字体（D14 + D28）。每条栈的**最后一项必须是通用族名**，
//! 否则干净安装的系统上会退化成豆腐块。
//!
//! ## 为什么要表达「这个族有哪几档字重」
//!
//! M3 Expressive 的强调档会要 **700**（title-medium/title-small/label-*）与 **500**
//! （display/headline/body-*）。而中文字体的字重覆盖差别很大：
//! **Windows 的 Microsoft YaHei UI 只有 400 / 700 两档**，没有 500；macOS 苹方到 600 就断了、
//! 没有 700；只有 Linux 常见的 Noto Sans CJK / 思源黑体（可变字体）才 100~900 全覆盖。
//!
//! 所以栈项不是「一个族名」，而是 `(族名, 字重覆盖)`（[`FontFace`]），并且提供一个纯函数
//! [`Platform::resolve_weight`] 说明「这一档字重最终落到哪个族、是原生还是降级」：
//!
//! | 结果 | 含义 |
//! |---|---|
//! | [`WeightMatch::Native`] | 该族有这一档（可变字体在区间内也算原生） |
//! | [`WeightMatch::Nearest`] | 该族没有这一档，取最近的静态档（视觉上是「淡一点/粗一点」，不假） |
//! | [`WeightMatch::Synthetic`] | 最近的档差 ≥2 级（如只有 400 却要 700）→ 引擎**合成加粗**，观感廉价 |
//!
//! **合成加粗是降级，不是正常路径**（用户令：不许拿合成加粗当「还原到位」）。
//! iced/cosmic-text 侧机制：`Font { family, weight, .. }` 由 cosmic-text 按 (family, weight)
//! 选面，同族缺档时选最近字重；glyphon 另可用 swash 的合成（fake bold）兜底。
//! 中文缺档时的降级顺序：**① 换下一个有该档的族 → ② 用可变字体的 weight 轴 → ③ 最近静态档
//! → ④ 最后才是合成加粗**（并且不要用 700 排长中文正文）。
//!
//! ## 待确认（D28 联动，不在本 mission 拍板）
//!
//! 将来从 CDN 下载中文字体时，「一个 Regular 文件」不够——至少要 400/500/700 三档子集，
//! 或者一个可变字体（体积更大）。这个差异登记在这里，等 D28 的字体下载 mission 决定。

#[cfg(test)]
mod tests;

/// 平台（决定字体族回退栈）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Windows 10/11。
    Windows,
    /// macOS 12+。
    MacOs,
    /// Linux（X11/Wayland，桌面发行版）。
    Linux,
}

/// 字重覆盖（一个族实际提供了哪些字重）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightCoverage {
    /// 可变字体：一个连续区间（weight 轴）。
    Variable {
        /// 最细一档。
        min: u16,
        /// 最粗一档。
        max: u16,
    },
    /// 静态字体：离散字重表。
    Static(&'static [u16]),
}

impl WeightCoverage {
    /// 是否原生覆盖某档字重。
    pub fn supports(&self, weight: u16) -> bool {
        match self {
            WeightCoverage::Variable { min, max } => (*min..=*max).contains(&weight),
            WeightCoverage::Static(list) => list.contains(&weight),
        }
    }

    /// 最近的可选字重（静态表取绝对值最近；可变字体夹取到区间内）。
    pub fn nearest(&self, weight: u16) -> u16 {
        match self {
            WeightCoverage::Variable { min, max } => weight.clamp(*min, *max),
            WeightCoverage::Static(list) => {
                let mut best = list[0];
                let mut best_delta = u16::MAX;
                for candidate in *list {
                    let delta = candidate.abs_diff(weight);
                    if delta < best_delta {
                        best = *candidate;
                        best_delta = delta;
                    }
                }
                best
            }
        }
    }
}

/// 一条字体栈项：族名 + 字重覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontFace {
    /// 族名（只写名字，不带路径/文件名）。
    pub family: &'static str,
    /// 该族提供的字重。
    pub weights: WeightCoverage,
}

impl FontFace {
    /// 构造。
    pub const fn new(family: &'static str, weights: WeightCoverage) -> Self {
        Self { family, weights }
    }

    /// 该族是否原生覆盖某档字重。
    pub fn supports(&self, weight: crate::typography::Weight) -> bool {
        self.weights.supports(weight.value())
    }
}

/// 字重匹配的质量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightMatch {
    /// 原生有这一档。
    Native,
    /// 同族最近档（差 <2 级）。
    Nearest,
    /// 差 ≥2 级，引擎只能合成加粗/减细（降级）。
    Synthetic,
}

/// 字重解析结果：最终用哪个族、哪个字重、是原生还是降级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontChoice {
    /// 选中的族名。
    pub family: &'static str,
    /// 请求的字重。
    pub requested: u16,
    /// 实际可用的字重。
    pub available: u16,
    /// 匹配质量。
    pub quality: WeightMatch,
}

impl FontChoice {
    /// 是否走了合成加粗（降级路径）。
    pub fn is_synthetic(&self) -> bool {
        self.quality == WeightMatch::Synthetic
    }
}

/// 字体族的用途分类（决定取哪条栈）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontFamily {
    /// 大字号展示（display/headline/title-large）：光学尺寸更大。
    Display,
    /// 正文（title/body/label）。
    Text,
    /// 等宽（日志、代码、版本号）。
    Mono,
}

// 字重覆盖来源等级：**B**（各平台字体规格/厂商文档；没有一份官方 token 文件能一次给全，
// 所以按「常见安装」标定，并用测试钉死——改值要连带改测试与协作文档）。
const W_VARIABLE_SEGOE: WeightCoverage = WeightCoverage::Variable { min: 300, max: 700 };
const W_VARIABLE_SF: WeightCoverage = WeightCoverage::Variable { min: 100, max: 900 };
const W_VARIABLE_NOTO: WeightCoverage = WeightCoverage::Variable { min: 100, max: 900 };
const W_VARIABLE_CASCADIA: WeightCoverage = WeightCoverage::Variable { min: 200, max: 700 };
const W_VARIABLE_JETBRAINS: WeightCoverage = WeightCoverage::Variable { min: 100, max: 800 };
const W_STATIC_SEGOE: WeightCoverage = WeightCoverage::Static(&[300, 400, 600, 700]);
const W_STATIC_YAHEI: WeightCoverage = WeightCoverage::Static(&[400, 700]);
const W_STATIC_HELVETICA: WeightCoverage = WeightCoverage::Static(&[300, 400, 500, 700]);
const W_STATIC_PINGFANG: WeightCoverage = WeightCoverage::Static(&[100, 200, 300, 400, 500, 600]);
const W_STATIC_MENLO: WeightCoverage = WeightCoverage::Static(&[400, 700]);
const W_STATIC_DEJAVU: WeightCoverage = WeightCoverage::Static(&[400, 700]);

// 各平台字体栈（写成 const 数组，好让 `font_stack` 的 `const fn` 直接返回 `&'static [_]`）。
const WIN_DISPLAY: [FontFace; 5] = [
    FontFace::new("Segoe UI Variable Display", W_VARIABLE_SEGOE),
    FontFace::new("Segoe UI", W_STATIC_SEGOE),
    FontFace::new("Microsoft YaHei UI", W_STATIC_YAHEI),
    FontFace::new("Microsoft YaHei", W_STATIC_YAHEI),
    FontFace::new("sans-serif", W_VARIABLE_NOTO),
];
const WIN_TEXT: [FontFace; 5] = [
    FontFace::new("Segoe UI Variable Text", W_VARIABLE_SEGOE),
    FontFace::new("Segoe UI", W_STATIC_SEGOE),
    FontFace::new("Microsoft YaHei UI", W_STATIC_YAHEI),
    FontFace::new("Microsoft YaHei", W_STATIC_YAHEI),
    FontFace::new("sans-serif", W_VARIABLE_NOTO),
];
const WIN_MONO: [FontFace; 4] = [
    FontFace::new("Cascadia Mono", W_VARIABLE_CASCADIA),
    FontFace::new("Consolas", W_STATIC_SEGOE),
    FontFace::new("Courier New", W_STATIC_MENLO),
    FontFace::new("monospace", W_STATIC_DEJAVU),
];
const MAC_DISPLAY: [FontFace; 4] = [
    FontFace::new("SF Pro Display", W_VARIABLE_SF),
    FontFace::new("Helvetica Neue", W_STATIC_HELVETICA),
    FontFace::new("PingFang SC", W_STATIC_PINGFANG),
    FontFace::new("sans-serif", W_VARIABLE_NOTO),
];
const MAC_TEXT: [FontFace; 4] = [
    FontFace::new("SF Pro Text", W_VARIABLE_SF),
    FontFace::new("Helvetica Neue", W_STATIC_HELVETICA),
    FontFace::new("PingFang SC", W_STATIC_PINGFANG),
    FontFace::new("sans-serif", W_VARIABLE_NOTO),
];
const MAC_MONO: [FontFace; 4] = [
    FontFace::new("SF Mono", W_VARIABLE_SF),
    FontFace::new("Menlo", W_STATIC_MENLO),
    FontFace::new("Monaco", W_STATIC_MENLO),
    FontFace::new("monospace", W_STATIC_DEJAVU),
];
const LINUX_SANS: [FontFace; 5] = [
    FontFace::new("Roboto", W_VARIABLE_NOTO),
    FontFace::new("Noto Sans", W_VARIABLE_NOTO),
    FontFace::new("Noto Sans CJK SC", W_VARIABLE_NOTO),
    FontFace::new("DejaVu Sans", W_STATIC_DEJAVU),
    FontFace::new("sans-serif", W_VARIABLE_NOTO),
];
const LINUX_MONO: [FontFace; 4] = [
    FontFace::new("JetBrains Mono", W_VARIABLE_JETBRAINS),
    FontFace::new("Noto Sans Mono", W_VARIABLE_NOTO),
    FontFace::new("DejaVu Sans Mono", W_STATIC_DEJAVU),
    FontFace::new("monospace", W_STATIC_DEJAVU),
];

impl Platform {
    /// 当前编译目标对应的平台（运行时用户覆盖请显式传 [`Platform`]）。
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Linux
        }
    }

    /// 稳定字符串名。
    pub const fn name(self) -> &'static str {
        match self {
            Platform::Windows => "windows",
            Platform::MacOs => "macos",
            Platform::Linux => "linux",
        }
    }

    /// 字体族回退栈（由首选到兜底，最后一项是通用族名）。
    pub const fn font_stack(self, family: FontFamily) -> &'static [FontFace] {
        match (self, family) {
            // Win10 起 Segoe UI 一定在；Win11 有光学尺寸分离的 Variable 版。
            (Platform::Windows, FontFamily::Display) => &WIN_DISPLAY,
            (Platform::Windows, FontFamily::Text) => &WIN_TEXT,
            (Platform::Windows, FontFamily::Mono) => &WIN_MONO,
            // macOS 12+：SF Pro 分 Display/Text 光学尺寸；中文走苹方（只到 600）。
            (Platform::MacOs, FontFamily::Display) => &MAC_DISPLAY,
            (Platform::MacOs, FontFamily::Text) => &MAC_TEXT,
            (Platform::MacOs, FontFamily::Mono) => &MAC_MONO,
            // Linux：不假设发行版预装什么，Roboto/Noto 是最常见的两套（Display 与 Text 同栈）。
            (Platform::Linux, FontFamily::Display | FontFamily::Text) => &LINUX_SANS,
            (Platform::Linux, FontFamily::Mono) => &LINUX_MONO,
        }
    }

    /// 该平台的**中文**兜底族（CJK 字形实际落到的那个）。
    ///
    /// 注意：栈里的 CJK 族只是「缺字形时的候选」，最终由文本引擎逐字形回退决定；
    /// 这个函数把它单独暴露出来，好让调用方按模块文档的降级顺序处理字重。
    pub const fn cjk_face(self) -> FontFace {
        match self {
            Platform::Windows => FontFace::new("Microsoft YaHei UI", W_STATIC_YAHEI),
            Platform::MacOs => FontFace::new("PingFang SC", W_STATIC_PINGFANG),
            Platform::Linux => FontFace::new("Noto Sans CJK SC", W_VARIABLE_NOTO),
        }
    }

    /// 按栈顺序解析某档字重的落点（纯函数，不查系统）。
    ///
    /// 规则：**优先原生**（栈里第一个真有的族）→ 都做不到时取首选族的最近档，
    /// 差距 ≥200 判为 [`WeightMatch::Synthetic`]。
    pub fn resolve_weight(
        self,
        family: FontFamily,
        weight: crate::typography::Weight,
    ) -> FontChoice {
        let stack = self.font_stack(family);
        let requested = weight.value();
        for face in stack {
            if face.weights.supports(requested) {
                return FontChoice {
                    family: face.family,
                    requested,
                    available: requested,
                    quality: WeightMatch::Native,
                };
            }
        }
        let first = stack[0];
        let available = first.weights.nearest(requested);
        FontChoice {
            family: first.family,
            requested,
            available,
            quality: if available.abs_diff(requested) >= 200 {
                WeightMatch::Synthetic
            } else {
                WeightMatch::Nearest
            },
        }
    }
}
