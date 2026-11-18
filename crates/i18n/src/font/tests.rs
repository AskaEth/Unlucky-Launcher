//! `i18n::font` 的单测（拆出来只为守 400 行硬线）。

use std::cell::Cell;

use super::*;

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
    assert_eq!(strategy.describe(), "cjk=needs-download(Noto Sans SC)");
}

/// pin 的形态守卫（M168）：URL 走 https、哈希是 64 位小写十六进制、字节数在大小上限内。
///
/// 抓得住：①有人把 URL 改成 `@main` 这类会漂移的引用（不是不行，但那样哈希必须跟着变，
/// 这条断言至少逼他看见自己在动什么）；②手抄哈希抄错一位（下一个人只会在真机上看到
/// 「校验失败」然后降级）；③把三个静态子集塞进来却没改字节数。
/// 抓不住：CDN 上今天还发不发得出这份字节（要真联网，报告里写明未验）。
#[test]
fn the_pinned_source_is_well_formed() {
    let Source::Pinned {
        urls,
        sha256,
        bytes,
    } = NOTO_SANS_SC.source;
    assert!(!urls.is_empty(), "至少要有一个 URL");
    for url in urls {
        assert!(url.starts_with("https://"), "字体必须走 https：{url}");
        assert!(
            !url.contains("@main"),
            "钉 commit 而不是会漂移的分支：{url}"
        );
    }
    assert_eq!(sha256.len(), 64, "{sha256}");
    assert!(
        sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{sha256}"
    );
    assert_eq!(sha256, sha256.to_ascii_lowercase(), "哈希用小写：{sha256}");
    assert!(bytes > 0 && bytes < 32 * 1024 * 1024, "{bytes}");
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
