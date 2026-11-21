//! 中文字体策略（D28 后半）：**只做判据与「该下载什么」，不联网**。
//!
//! ## 三句话
//!
//! 1. **英文环境不要字体**：`Lang::En` → [`Strategy::NotNeeded`]，与系统里装了什么无关。
//!    D28 原话是「纯英文环境不用管（零额外体积/零网络）」，这条是本模块最重要的断言。
//! 2. **中文环境先扫系统**：候选族名单见 [`CJK_FAMILIES`]，命中即用（[`Strategy::System`]）。
//! 3. **扫不到才说要下载**：[`Strategy::NeedsDownload`] 是一个**信号**，不是动作——
//!    真正下载由注入的 [`Fetcher`] 实现（M168 起的真实现是
//!    `crates/app-iced/src/fonts/cdn.rs`，底层走 `unlucky_core::download::font`）。本 crate 保持
//!    **零依赖**：它只携带「下什么」（[`Source::Pinned`]：URL 候选 + sha256 + 字节数），不碰网络。
//!
//! ## 谁在什么时刻问这里
//!
//! | 调用点 | 时机 | 后果 |
//! |---|---|---|
//! | `crates/app-iced/src/fonts.rs::resolve` | 启动（按系统 locale） | 打一行 `cjk=…` 日志 |
//! | `crates/app-iced/src/fonts.rs::cjk_for` | 设置页切语言 | 复算 + 需要时**触发下载** |
//!
//! 两条路都**只**在 [`Strategy::NeedsDownload`] 时才碰 [`Fetcher`]（[`ensure`] 的判据），
//! 英文环境与「系统已有字体」两条路一次网络调用都没有（守卫见本文件的单测与
//! `crates/app-iced/src/fonts/install/tests.rs`）。
//!
//! ## 诚实边界（抓得住什么 / 抓不住什么）
//!
//! - **抓得住**：语言 → 是否需要中文 → 系统里有没有候选族 → 要不要下载 这条链路的判定；
//! - **抓不住**：①**族名存在 ≠ 字形覆盖**——`fontdb` 里挂着一个叫 `SimSun` 的族也可能是个残缺子集，
//!   真判据要用 `ttf-parser` 查 cmap（M60 Q3 的 spike 做过，本 crate 不引依赖）；
//!   ②下载下来的字节能不能在**这台机器**的渲染器上正常出字（注册与回退在 app-iced；macOS/Linux
//!   没有真机验证，见 M168 报告）。

use crate::lang::Lang;

/// 已知的系统 CJK 族（Windows / macOS / Linux，按「观感优先」排序，命中第一个就用）。
///
/// 只收**简体**优先的常见族；`Microsoft JhengHei` 这类繁体族留在名单里垫底——
/// 仓内只有一套中文目录（简体），繁体用户看简体的观感仍好过看英文。
pub const CJK_FAMILIES: &[&str] = &[
    // Windows 10/11
    "Microsoft YaHei UI",
    "Microsoft YaHei",
    "DengXian",
    "SimHei",
    "SimSun",
    "Microsoft JhengHei",
    // macOS
    "PingFang SC",
    "Hiragino Sans GB",
    "Heiti SC",
    "Songti SC",
    // Linux
    "Noto Sans CJK SC",
    "Noto Sans SC",
    "Source Han Sans SC",
    "Source Han Sans CN",
    "Sarasa Gothic SC",
    "WenQuanYi Micro Hei",
    "Droid Sans Fallback",
];

/// 中文字体必须覆盖的字重（M-决定日志 ③：Android 16 的可变思源黑体缺 100–400，
/// 而 M3 Expressive 的 emphasized 档要 700 ⇒ 「400 / 500 / 700 至少三档」）。
pub const REQUIRED_WEIGHTS: &[u16] = &[400, 500, 700];

/// 可下载字体的来源：**已钉死**的 URL 候选 + `sha256` + 字节数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// 钉死的一套下载参数（M168）。
    ///
    /// 为什么是「常量 + 多重校验」而不是「让下载器自己挑」：CDN 上的字节会变（上游改字体、
    /// 镜像换了实现），而**字体是注册进渲染器的可执行内容**。所以 URL 钉到 **git commit**
    /// （不是 `@main`）、字节数与 `sha256` 一起钉，任何一项对不上就拒绝落盘（见
    /// `unlucky_core::download::font`）。
    ///
    /// `urls` 是**候选**：首选挂了才试下一个。两个候选**必须是不同的失败域**（Cloudflare 侧 +
    /// Fastly 侧，见 [`NOTO_SANS_SC`] 的实测表）；同一家 CDN 的两个域名不算冗余。
    ///
    /// 每次尝试与整条取回都有整体时限（`unlucky_core::download::font` 的 `ATTEMPT_DEADLINE` /
    /// `TOTAL_DEADLINE`）：慢而不死的连接（IPv6 爬行实测 67 KB/s）会在时限处被打断、转下一个
    /// 候选，而不是让用户在设置页干等四分钟。超时归「网络」档，界面文案是「网络不通或太慢」。
    Pinned {
        /// URL 候选（按顺序尝试）。
        urls: &'static [&'static str],
        /// 期望 `sha256`（小写十六进制，64 位）。
        sha256: &'static str,
        /// 期望字节数（进度预算 + 大小上限判据）。
        bytes: u64,
    },
}

/// 一套可下载的 CJK 字体描述（不含 URL / 哈希，原因见 [`Source`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CjkFont {
    /// 族名（下载并注册后，`Font` 用它取字）。
    pub family: &'static str,
    /// 许可证（必须是 OFL / Apache 这类可再分发的；D28 明令不得内置 Segoe）。
    pub license: &'static str,
    /// 这套字体可用的字重。
    pub weights: &'static [u16],
    /// 下载源（URL 候选 + sha256 + 字节数，见 [`Source::Pinned`]）。
    pub source: Source,
}

/// 缺字体时的默认方案：Noto Sans SC（OFL-1.1）**可变字体**（wght 100–900，整个简体字符集）。
///
/// 钉死的东西（M168 实测抓的，不是猜的）：
///
/// | 项 | 值 |
/// |---|---|
/// | 上游 | `google/fonts` 仓库的 `ofl/notosanssc/NotoSansSC[wght].ttf` |
/// | 版本 | commit `2894aab31764f10f29c421bdfd2340d3b382d384`（该文件的最后一次改动，2022-12-09） |
/// | 字节数 | 17 772 300（16.95 MiB） |
/// | sha256 | `a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da` |
///
/// 为什么钉 commit 而不是 `@main`：`@main` 会随上游改动漂移，哈希一漂就成了「每次中文环境都下
/// 一份、然后校验失败、然后降级」——比不下更糟。commit 是不变内容，哈希因此是可复现的判据。
///
/// 为什么是**一个**可变字体而不是三个静态子集（400/500/700 各一份）：三份静态子集合计 ≈25 MiB，
/// 而可变字体一份 16.9 MiB 就覆盖 100–900（cosmic-text 0.15 会把请求的字重写进 `wght` 轴，见
/// `crates/core/src/download/font.rs` 模块文档与 M168 报告里的「抓不住」一段）。
pub const NOTO_SANS_SC: CjkFont = CjkFont {
    family: "Noto Sans SC",
    license: "OFL-1.1",
    weights: &[100, 200, 300, 400, 500, 600, 700, 800, 900],
    source: Source::Pinned {
        // 两个候选**必须是不同的失败域**（M168 复审 P2-1）：`cdn.jsdelivr.net` 与
        // `fastly.jsdelivr.net` 都 CNAME 到 `jsdelivr.map.fastly.net`（同一批 Fastly 边缘），
        // 「一个坏了换另一个」等于没有冗余——上一版就是这样，被复审当场打掉。
        //
        // 实测（两份独立的国内网络取证，同一份 17 772 300 B 内容，sha256 都是 a3041811…）：
        //
        // | 域名 | 解析 | 本机（本 worktree 开发机） | 复审机（无代理） |
        // |---|---|---|---|
        // | gcore.jsdelivr.net | Cloudflare 104.16.x | 13.3 s 完整 | 16.6 s 完整 |
        // | testingcf.jsdelivr.net | Cloudflare 104.16.x | 11.8 s 完整 | 14.8 s 完整 |
        // | cdn.jsdelivr.net | Fastly 151.101.x | 11.9 s 完整（另两次 5.7 s / 14.9 s）| 1.1 s，但**走 IPv6 时 3 次都掉到 ≈67 KB/s**（60 s 只到 4.0 MB）|
        // | fastly.jsdelivr.net | 同上（Fastly）| — | 20 s 只到 2.5 MB（比它「要备的那条」还慢）|
        //
        // 排序依据（不是「听说更稳」）：**首选放 Cloudflare 那一侧**——Fastly 侧的 IPv6 边缘
        // 在复审环境里会掉进 60~77 KB/s 的慢路径（那条路上 17.7 MB 要 ≈4 分钟），而 Cloudflare
        // 侧两份取证都完整下完、且是独立的失败域。二选留 Fastly 侧：本机它反而最快（5.7~11.9 s），
        // 保留它是为了「Cloudflare 一侧整体不可用时还有另一家 CDN」。
        // 先后只影响速度，不影响正确性：sha256 只认一份，镜像给了别的内容会被当场拒。
        urls: &[
            "https://gcore.jsdelivr.net/gh/google/fonts@2894aab31764f10f29c421bdfd2340d3b382d384/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf",
            "https://cdn.jsdelivr.net/gh/google/fonts@2894aab31764f10f29c421bdfd2340d3b382d384/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf",
        ],
        sha256: "a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da",
        bytes: 17_772_300,
    },
};

/// 字体策略结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// 不需要中文字体（英文环境）。
    NotNeeded,
    /// 系统里已有可用族，直接用。
    System {
        /// 命中的族名。
        family: &'static str,
    },
    /// 需要下载（本模块只发信号，不联网）。
    NeedsDownload(&'static CjkFont),
    /// 探测不出（拿不到系统字体库）——**不等于**「需下载」：不许凭猜测去下载。
    Unprobeable,
}

impl Strategy {
    /// 一行日志片段（`cjk=system:"Microsoft YaHei"` / `cjk=needs-download(Noto Sans SC)` …）。
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Strategy::NotNeeded => "cjk=not-needed".to_owned(),
            Strategy::System { family } => format!("cjk=system:\"{family}\""),
            Strategy::NeedsDownload(font) => format!("cjk=needs-download({})", font.family),
            Strategy::Unprobeable => "cjk=unprobeable".to_owned(),
        }
    }
}

/// 判据（纯函数）：语言 + 「某族装没装」→ 策略。
///
/// `installed` 由调用方提供（iced 版查系统字体库，单测给假闭包）。
#[must_use]
pub fn decide(lang: Lang, installed: &dyn Fn(&str) -> bool) -> Strategy {
    if lang != Lang::ZhCn {
        // D28：纯英文环境不加载任何中文字体，也不下载。
        return Strategy::NotNeeded;
    }

    CJK_FAMILIES
        .iter()
        .copied()
        .find(|family| installed(family))
        .map_or(Strategy::NeedsDownload(&NOTO_SANS_SC), |family| {
            Strategy::System { family }
        })
}

/// 字体下载器接口（**注入式**：i18n 不碰网络，真实现由 app 层提供）。
pub trait Fetcher {
    /// 取一套字体（返回原始字节，装进字体系统是调用方的事）。
    ///
    /// 契约：**同一个进程里第二次调用应当零网络**（缓存命中），且返回的字节已通过校验
    /// （实现方的责任，见 `unlucky_core::download::font`）。
    ///
    /// # Errors
    /// 网络失败 / 校验失败 / 磁盘失败三类人话错误。
    fn fetch(&self, font: &'static CjkFont) -> Result<Vec<u8>, FetchError>;
}

/// 下载失败的分类（给 UI 决定要不要重试；分类少是故意的）。
///
/// 三类对应三句不同的话：网络类「过会儿再试」、校验类「这个源给的东西不对」、磁盘类
/// 「腾点空间」。UI 文案按 [`FetchError::class`] 选词，**不要**按错误串猜。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// 网络层失败（超时 / 连不上 / 非 2xx）。
    Network(String),
    /// 校验失败（哈希 / 长度不符，或不是字体文件）。
    Integrity(String),
    /// 本地磁盘失败（建目录 / 写文件 / 换名）。
    Disk(String),
}

impl FetchError {
    /// 稳定类别名（`network` / `integrity` / `disk`；日志与 i18n 金表按它选词）。
    #[must_use]
    pub const fn class(&self) -> &'static str {
        match self {
            Self::Network(_) => "network",
            Self::Integrity(_) => "integrity",
            Self::Disk(_) => "disk",
        }
    }

    /// 人类可读描述。
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Network(message) | Self::Integrity(message) | Self::Disk(message) => message,
        }
    }
}

/// [`ensure`] 的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// 本来就不需要（英文环境）。
    NothingToDo,
    /// 系统已有，直接用。
    AlreadySystem {
        /// 命中的族名。
        family: &'static str,
    },
    /// 下载成功，拿到字节（交给调用方注册）。
    Downloaded(Vec<u8>),
    /// 探测不出，什么都没做。
    Unknown,
}

/// 按策略执行：需要下载时才碰 [`Fetcher`]。
///
/// # Errors
/// 下载器返回的错误原样透出（不吞、不改写分类）。
pub fn ensure(strategy: Strategy, fetcher: &dyn Fetcher) -> Result<Fetched, FetchError> {
    match strategy {
        Strategy::NotNeeded => Ok(Fetched::NothingToDo),
        Strategy::System { family } => Ok(Fetched::AlreadySystem { family }),
        Strategy::Unprobeable => Ok(Fetched::Unknown),
        Strategy::NeedsDownload(font) => fetcher.fetch(font).map(Fetched::Downloaded),
    }
}

#[cfg(test)]
#[path = "font/tests.rs"]
mod tests;
