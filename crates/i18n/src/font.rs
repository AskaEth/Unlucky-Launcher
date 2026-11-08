//! 中文字体策略（D28 后半）：**只做判据，不联网**。
//!
//! ## 三句话
//!
//! 1. **英文环境不要字体**：`Lang::En` → [`Strategy::NotNeeded`]，与系统里装了什么无关。
//!    D28 原话是「纯英文环境不用管（零额外体积/零网络）」，这条是本模块最重要的断言。
//! 2. **中文环境先扫系统**：候选族名单见 [`CJK_FAMILIES`]，命中即用（[`Strategy::System`]）。
//! 3. **扫不到才说要下载**：[`Strategy::NeedsDownload`] 是一个**信号**，不是动作——
//!    真正下载由注入的 [`Fetcher`] 实现，本 mission 不写网络实现（D28 的 CDN 下载是后续 mission）。
//!
//! ## 扫描为什么在 UI 壳层而不是这里
//!
//! 真扫系统字体要 FontConfig / DirectWrite / iced 的 `font_system()`——任何一条都会把 iced 或平台
//! SDK 拉进本 crate，破掉零依赖纪律。所以本模块只收一个 `installed: &dyn Fn(&str) -> bool`
//! （iced 版实现在 `crates/app-iced/src/fonts/cjk.rs`），单测用假闭包，脱机可跑。
//!
//! ## 诚实边界（抓得住什么 / 抓不住什么）
//!
//! - **抓得住**：语言 → 是否需要中文 → 系统里有没有候选族 → 要不要下载 这条链路的判定；
//! - **抓不住**：①**族名存在 ≠ 字形覆盖**——`fontdb` 里挂着一个叫 `SimSun` 的族也可能是个残缺子集，
//!   真判据要用 `ttf-parser` 查 cmap（M60 Q3 的 spike 做过，本 mission 不引依赖）；
//!   ②下载下来的字节怎么注册进 iced 的字体系统——iced 0.14 有 `iced::font::load(bytes) -> Task`
//!   可在运行时注册（已核对上游源码），但「什么时候调、失败怎么回退、和启动日志怎么对账」
//!   属于下载 mission，本 crate 不碰。

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

/// 可下载字体的来源。本 mission **不 pin URL/哈希**——见 [`Source::PendingPin`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// 待下载 mission 钉死：确切 URL + `sha256` + 子集范围（简体常用字 + 标点），
    /// 并按 [`REQUIRED_WEIGHTS`] 覆盖 400/500/700。**故意不写一个随手想出来的 URL**：
    /// 猜错的 CDN 路径会被后面所有人当事实用。
    PendingPin,
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
    /// 下载源（尚未钉死）。
    pub source: Source,
}

/// 缺字体时的默认方案：Noto Sans SC（OFL-1.1）。
pub const NOTO_SANS_SC: CjkFont = CjkFont {
    family: "Noto Sans SC",
    license: "OFL-1.1",
    weights: &[300, 400, 500, 700, 900],
    source: Source::PendingPin,
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

/// 字体下载器接口（**注入式**；本 mission 没有网络实现）。
pub trait Fetcher {
    /// 取一套字体（返回原始字节，装进字体系统是调用方的事）。
    ///
    /// # Errors
    /// 网络失败 / 校验失败等人话错误。
    fn fetch(&self, font: &'static CjkFont) -> Result<Vec<u8>, FetchError>;
}

/// 下载失败的分类（给 UI 决定要不要重试；分类少是故意的）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// 网络层失败（超时 / 连不上 / 非 2xx）。
    Network(String),
    /// 校验失败（哈希 / 长度不符，或不是字体文件）。
    Integrity(String),
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
mod tests {
    use super::*;
    use std::cell::Cell;

    fn only(installed: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |family| installed.contains(&family)
    }

    /// 假下载器：记录调用次数，避免「悄悄联网」这种事在单测里被放过。
    struct FakeFetcher {
        calls: Cell<usize>,
        result: Result<Vec<u8>, FetchError>,
        seen: Cell<Option<&'static str>>,
    }

    impl FakeFetcher {
        fn new(result: Result<Vec<u8>, FetchError>) -> Self {
            FakeFetcher {
                calls: Cell::new(0),
                result,
                seen: Cell::new(None),
            }
        }
    }

    impl Fetcher for FakeFetcher {
        fn fetch(&self, font: &'static CjkFont) -> Result<Vec<u8>, FetchError> {
            self.calls.set(self.calls.get() + 1);
            self.seen.set(Some(font.family));
            self.result.clone()
        }
    }

    #[test]
    fn english_never_wants_a_cjk_font() {
        // 抓得住：D28 的核心承诺——英文环境不加载/不下载中文字体（D28 说「零额外体积/零网络」）。
        // 抓不住：程序真的没去读那个字体文件（那是加载器的行为，本 crate 只管判据）。
        assert_eq!(decide(Lang::En, &only(&[])), Strategy::NotNeeded);
        assert_eq!(
            decide(Lang::En, &only(&["Microsoft YaHei"])),
            Strategy::NotNeeded,
            "装了中文字体也不该进入中文分支"
        );
    }

    #[test]
    fn chinese_prefers_the_first_installed_candidate() {
        // 抓得住：候选顺序（YaHei UI > YaHei > SimSun…）被调换，或命中判断写成了「装了任意字面就 ok」。
        // 抓不住：命中的族是否真有简体字形覆盖（族名存在 ≠ cmap 覆盖，见模块文档）。
        assert_eq!(
            decide(
                Lang::ZhCn,
                &only(&["Segoe UI", "Microsoft YaHei", "SimSun"])
            ),
            Strategy::System {
                family: "Microsoft YaHei"
            }
        );
        assert_eq!(
            decide(Lang::ZhCn, &only(&["PingFang SC"])),
            Strategy::System {
                family: "PingFang SC"
            }
        );
        assert_eq!(
            decide(Lang::ZhCn, &only(&["Noto Sans CJK SC", "Noto Sans SC"])),
            Strategy::System {
                family: "Noto Sans CJK SC"
            }
        );
    }

    #[test]
    fn chinese_without_cjk_fonts_emits_the_download_signal() {
        // 抓得住：中文环境 + 系统无 CJK 时**给信号**（而不是静默回英文、也不是自己联网）。
        // 抓不住：信号之后 UI 有没有把提示画出来（页面侧 / 截图验收）。
        let strategy = decide(Lang::ZhCn, &only(&["Segoe UI", "Roboto", "Cascadia Mono"]));
        let Strategy::NeedsDownload(font) = strategy else {
            panic!("应当要求下载：{strategy:?}");
        };
        assert_eq!(font.family, "Noto Sans SC");
        assert_eq!(font.license, "OFL-1.1");
        assert_eq!(font.source, Source::PendingPin, "URL/哈希留给下载 mission");
        assert_eq!(strategy.describe(), "cjk=needs-download(Noto Sans SC)");
    }

    #[test]
    fn download_fonts_cover_the_required_weights() {
        // 抓得住：将来换了字体却没覆盖 400/500/700（M3 Expressive 的 emphasized 档要 700）。
        // 抓不住：字重是可变轴还是静态子集（那要看下载 mission 选的文件，本 crate 只有元数据）。
        for weight in REQUIRED_WEIGHTS {
            assert!(
                NOTO_SANS_SC.weights.contains(weight),
                "缺 {weight} 档：{:?}",
                NOTO_SANS_SC.weights
            );
        }
    }

    #[test]
    fn ensure_only_touches_the_fetcher_when_download_is_needed() {
        // 抓得住：英文环境/系统已有字体时**仍然调了下载器**（= 偷跑网络请求）；
        // 也抓得住下载失败被吞掉（错误必须原样透出）。
        // 抓不住：真实现的网络行为（本 crate 没有网络实现，这正是不联网的证据）。
        let idle = FakeFetcher::new(Ok(vec![1, 2, 3]));
        assert_eq!(ensure(Strategy::NotNeeded, &idle), Ok(Fetched::NothingToDo));
        assert_eq!(
            ensure(Strategy::System { family: "SimSun" }, &idle),
            Ok(Fetched::AlreadySystem { family: "SimSun" })
        );
        assert_eq!(ensure(Strategy::Unprobeable, &idle), Ok(Fetched::Unknown));
        assert_eq!(idle.calls.get(), 0, "这三种情况都不许联网");

        let ok = FakeFetcher::new(Ok(vec![9; 4]));
        assert_eq!(
            ensure(Strategy::NeedsDownload(&NOTO_SANS_SC), &ok),
            Ok(Fetched::Downloaded(vec![9; 4]))
        );
        assert_eq!(ok.calls.get(), 1);
        assert_eq!(ok.seen.get(), Some("Noto Sans SC"));

        let broken = FakeFetcher::new(Err(FetchError::Network("超时".to_owned())));
        assert_eq!(
            ensure(Strategy::NeedsDownload(&NOTO_SANS_SC), &broken),
            Err(FetchError::Network("超时".to_owned()))
        );
    }

    #[test]
    fn strategy_describe_is_one_line_for_every_case() {
        // 抓得住：日志片段里混进换行（启动日志是按行解析的）。
        // 抓不住：这些片段是否真的被打印（app-iced 的启动路径在那边测）。
        let cases = [
            Strategy::NotNeeded,
            Strategy::System { family: "SimSun" },
            Strategy::NeedsDownload(&NOTO_SANS_SC),
            Strategy::Unprobeable,
        ];
        for case in cases {
            let line = case.describe();
            assert_eq!(line.lines().count(), 1, "{line}");
            assert!(line.starts_with("cjk="), "{line}");
        }
        assert_eq!(Strategy::NotNeeded.describe(), "cjk=not-needed");
        assert_eq!(Strategy::Unprobeable.describe(), "cjk=unprobeable");
    }
}
