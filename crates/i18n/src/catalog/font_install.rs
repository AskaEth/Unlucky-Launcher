//! 中文字体**下载状态行**的文案（M168）：设置页 General 组那一行的四态文字 + 三类失败原因。
//!
//! ## 为什么单开一个域文件（而不是塞进 `font.rs`）
//!
//! `catalog/font.rs` 那四条（`FontCjkNeedsDownload` / `FontCjkDownloading` / `FontCjkDownloadFailed`
//! / `FontCjkReady`）是 M92「只发信号」时代的文案，位置在装配链**中间**。往那个域后面追加会把
//! 其后所有 key 在 `Key::ALL` 里整体平移一位 ⇒ 金表（逐 index 逐字节）整片红。
//!
//! 本域挂在**链尾**（`update_entries` 之后），于是 `Key::ALL` 的既有顺序一个不动，金表只需在
//! 末尾追加（M104 / M106 / M107 / M109 / M114 / M159 都是这条做法）。
//!
//! ## 四条状态各一句话（tower 定死的口径）
//!
//! | 状态 | 文案说什么 |
//! |---|---|
//! | 下载中 | 「正在获取中文字体…（16.9 MB）」——**没有**假进度条，有真实量（字节数）才写 |
//! | 完成 | 「中文字体已就绪：⟨族名⟩」（复用 `FontCjkReady`） |
//! | 失败 | 「获取失败（⟨三类原因之一⟩）。界面照常可用，用的是系统字体；再切一次语言可以重试」 |
//! | 无需下载 | 系统已有字体 / 当前语言不需要，两条分开说（对用户是两件事） |
//!
//! 「探测不出」也要有话说：那是「不知道」，既不是「没有」也不是「不需要」——写清楚「未下载」，
//! 免得用户以为我们偷偷下过一份。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! font_install_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 中文字体下载状态行（M168：设置页 General 组）
            FontCjkStatusLabel => ("Chinese font", "中文字体"),
            FontCjkStatusNotNeeded => (
                "Not needed for this interface language",
                "当前界面语言不需要中文字体"
            ),
            FontCjkStatusSystem => ("Ready (system font: {family})", "已就绪（系统字体：{family}）"),
            FontCjkStatusDownloading => (
                "Fetching a Chinese font… ({size})",
                "正在获取中文字体…（{size}）"
            ),
            FontCjkStatusFailed => (
                "Download failed ({reason}). The interface keeps working with system fonts; switch the language again to retry.",
                "获取失败（{reason}）。界面照常可用，用的是系统字体；再切一次语言可以重试。"
            ),
            // 超时也归这一档（M168 复审 P2-2）：用户要分清「太慢/没网」与「内容不对」，
            // 但不需要知道我们内部把它叫 Timeout 还是 Network。
            FontCjkReasonNetwork => (
                "network unreachable or too slow (timed out)",
                "网络不通或太慢（超时）"
            ),
            FontCjkReasonIntegrity => (
                "the mirror sent content that failed verification",
                "镜像内容校验不通过"
            ),
            FontCjkReasonDisk => ("cannot write to the data directory", "写不进数据目录"),
            FontCjkStatusUnknown => (
                "Not determined yet; nothing was downloaded",
                "尚未判定；没有下载任何字体"
            ),
            FontCjkStatusUnprobeable => (
                "System fonts cannot be probed, so nothing was downloaded",
                "无法探测系统字体，因此没有下载"
            ),
            FontCjkStatusHint => (
                "Downloaded once into the data directory and cached; the next start does not download it again. A mirror can be set with ui.font_cdn.",
                "只下这一次，缓存在数据目录里，下次启动不重下。镜像可用配置键 ui.font_cdn 指定。"
            ),
        } }
    };
}

pub(crate) use font_install_entries;
