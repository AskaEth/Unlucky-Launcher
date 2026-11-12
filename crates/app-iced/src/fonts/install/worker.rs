//! 下载线程与字体注册（M168）：`install.rs` 的「动作」那一半。
//!
//! ## 为什么拆出来
//!
//! `install.rs` 是「状态 + 判据 + 触发」，已经贴着 400 行硬线。这里只放真正干活的两件：
//!
//! 1. [`start`]：起一条后台线程（进程内同时只有一条），把「正在下载 → 结果」写回状态槽；
//! 2. [`register_into_font_system`]：把字节交给 iced 的全局字体系统（运行时注册）。
//!
//! ## 为什么下载在后台线程
//!
//! 一份 16.9 MiB 的字体在弱网下要几十秒，而触发点（设置页切语言）在 UI 线程上。
//! 在 UI 线程同步下完 = 窗口几十秒不响应。
//!
//! ## 线程里做了什么（顺序即判据）
//!
//! ```text
//! CdnFetcher（缓存命中 ⇒ 零网络；否则共享引擎下载 + sha256 复验 + 整体时限）
//!   → i18n::font::ensure 只有 NeedsDownload 才调 fetcher
//!   → 成功：注册进 iced，状态 Ready{family, from_cache}
//!   → 失败：状态 Failed{class}，**什么都不注册**（降级回系统字体，界面照常可用）
//! ```
//!
//! ## panic 这件事分两档说（复审 r3 P2-1）
//!
//! | 档 | panic 语义 | 线程 panic 之后 |
//! |---|---|---|
//! | `release`（`Cargo.toml`：`panic = "abort"`，发布走它） | 进程 abort | 状态槽随进程消失（**不会**停在 Downloading）；但「降级回系统字体」**也不存在**——启动器整体退出 |
//! | `dev` / `test` / `fast`（unwind） | 栈展开 | [`run`] 里的 `catch_unwind` 把状态写成 `Failed` |
//!
//! 所以 `catch_unwind` 是**给 unwind 档的**，不能拿它给「下载失败不崩」背书：发布版在注册路径
//! panic 时仍会 abort。要让发布版也降级，得让注册路径不 panic（或改成 `panic = "unwind"`，
//! 那属于发布 profile 的决定，不在本 mission）。
//!
//! ## 诚实边界
//!
//! - **抓得住**：状态流转、失败不注册、日志三行（start / ok / failed）；
//! - **抓不住**：真机上字体注册之后**出字的样子**（cosmic-text 用 `wght` 轴选字重这件事只核对了
//!   上游源码，没有真机截图对照；macOS / Linux 完全没有真机），也抓不住发布档的 abort 行为
//!   （那要真跑一个 `--release` 的 panic 用例）。

use unlucky_i18n::font::{self, Strategy};

use super::{ensure_for, update, CjkInstall, FetchClass};
use crate::fonts::cdn::CdnFetcher;
use crate::platform::data;

/// 起一条后台下载（进程内同时只允许一条：`install::after_probe` 负责那道闸）。
///
/// 状态先落 `Downloading` 再起线程；**线程起不来时**立刻落 `Failed`（用 `Builder::spawn` 而不是
/// 裸 `thread::spawn`：后者失败会 panic，而发布档 `panic = "abort"` ⇒ 整个启动器退出）。
pub(super) fn start(font: &'static font::CjkFont) {
    let font::Source::Pinned { bytes, .. } = font.source;
    let Some(cache_root) = data::cache_root() else {
        update(CjkInstall::Failed {
            class: FetchClass::Disk,
            message: "数据根解析不出来，字体无处可落".to_owned(),
        });
        return;
    };

    let fetcher = match CdnFetcher::new(font, cache_root) {
        Ok(fetcher) => fetcher,
        Err(error) => {
            update(CjkInstall::Failed {
                class: FetchClass::from(&error),
                message: error.message().to_owned(),
            });
            return;
        }
    };

    let cache_file = fetcher.cache_file();
    update(CjkInstall::Downloading { bytes });
    eprintln!(
        "[font] cjk-download start family=\"{}\" bytes={bytes} file={} cache_present={}",
        font.family,
        cache_file.display(),
        // 这条只是「文件在不在」（诊断用），**不是** from_cache：坏缓存会被删掉重下，
        // 真正的命中结论在 `fetcher.was_cached()`（取完之后）。
        fetcher.has_cached()
    );

    let spawned = std::thread::Builder::new()
        .name("ull-cjk-font".to_owned())
        .spawn(move || run(font, fetcher));

    if let Err(error) = spawned {
        update(CjkInstall::Failed {
            class: FetchClass::Disk,
            message: format!("起不了字体下载线程（{error}）"),
        });
    }
}

/// 线程体：取字体 → 注册 → 写终态（**一定**写终态，见模块文档「panic 这件事分两档说」）。
fn run(font: &'static font::CjkFont, fetcher: CdnFetcher) {
    let state = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ensure_for(
            Strategy::NeedsDownload(font),
            &fetcher,
            &register_into_font_system,
        )
    })) {
        Ok(state) => state,
        Err(_) => CjkInstall::Failed {
            class: FetchClass::Disk,
            message: "字体安装线程 panic（已捕获，状态回到终态；界面继续用系统字体）".to_owned(),
        },
    };

    // `from_cache` 只认下载器**这次**的结论（复审 r1 次要项）：取之前「文件在不在」会说谎——
    // 坏缓存会被删掉重下，那时 `has_cached()` 还是 true，而真相是「联网下了一次」。
    let from_cache = fetcher.was_cached().unwrap_or(false);
    let state = match state {
        CjkInstall::Ready { family, .. } => CjkInstall::Ready { family, from_cache },
        other => other,
    };

    match &state {
        CjkInstall::Ready { family, from_cache } => eprintln!(
            "[font] cjk-download ok family=\"{family}\" from_cache={from_cache} \
             （中文字形现在由它兜底，界面不需要重启）"
        ),
        CjkInstall::Failed { class, message } => eprintln!(
            "[font] cjk-download failed class={} {message}（降级：继续用系统字体，界面照常可用）",
            class.as_str()
        ),
        other => eprintln!("[font] cjk-download {}", other.describe()),
    }
    update(state);
}

/// 把字体字节注册进 iced 的**全局字体系统**（`iced_graphics` 的 `FontSystem::load_font`）。
///
/// 返回注册后的族名（从字体库里读回来的）：变量字体的 `name` 表里族名是 `Noto Sans SC`、
/// 而默认实例是 Thin，所以**不能**假设「注册了就叫 NOTO_SANS_SC.family」——这里读回来核对。
///
/// # Errors
/// 字体系统锁被毒掉、或注册后字体库里找不到那个族（字节不是字体 / `fontdb` 不认）。
pub fn register_into_font_system(bytes: Vec<u8>) -> Result<String, String> {
    let lock = iced::advanced::graphics::text::font_system();
    let mut font_system = lock
        .write()
        .map_err(|_| "字体系统的读写锁被毒掉（另一处注册 panic 过）".to_owned())?;

    font_system.load_font(std::borrow::Cow::Owned(bytes));

    // 先把族名抄出来再判：`faces()` 的迭代器借着 `font_system`，直接链到 `ok_or_else` 会让
    // 那个借用活过守卫（E0597）。
    let families: Vec<String> = font_system
        .raw()
        .db()
        .faces()
        .flat_map(|face| face.families.iter())
        .map(|(family, _)| family.clone())
        .collect();

    families
        .iter()
        .find(|family| family.as_str() == font::NOTO_SANS_SC.family)
        .cloned()
        .ok_or_else(|| format!("注册后在字体库里找不到 \"{}\"", font::NOTO_SANS_SC.family))
}
