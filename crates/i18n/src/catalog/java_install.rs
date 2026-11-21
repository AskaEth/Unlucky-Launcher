//! 「下载 Java {major}」那个出口的文案（M334，接在 M331 的「Java 不符」框上）。
//!
//! 背景（台账 P38 / 主人 Q10 的原话口径）：M331 让启动前那个框能说「这个版本要用另一个
//! Java」，可它只给得出两个出口——「去设置改 Java」与「仍要启动」。手里只有 17/21 的用户
//! 去设置也变不出 Java 8，于是第三个出口是**直接装一个**：这个域就是它的话。
//!
//! 三条口径：
//!
//! 1. **下载中是「一句话 + 数字」，不是空圈**：`JavaGuardDownloading` 带上 `{major}`，
//!    用户要知道自己等的是哪一个。
//! 2. **失败必须具名**：`JavaGuardDownloadFailed` 的 `{reason}` 里装的是**厂商 + core 原文**
//!    （发行版名走本域的 `JavaVendor*` 文案，机器细节如 HTTP 状态码保留原文——那是取证材料）。
//! 3. **装完要给下一步**：`JavaGuardDownloadDone` + `JavaGuardRetryLaunch`——装好了却说不出
//!    「现在点哪儿」，等于让用户自己猜那个框还关不关。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节；本域**追加在链尾**
//! （`java_guard_entries` 之后），于是 `Key::ALL` 顺序 = 金表顺序 = 先 main 的全部域，
//! 再 `pack_java` / `pack_processor` / `java_guard`，最后本域（金表只追加不重排）。

macro_rules! java_install_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 启动被拦下后的第三个出口：装一个对版本的 Java（M334）
            JavaGuardDownload => ("Download Java {major}", "下载 Java {major}"),
            JavaGuardDownloading => (
                "Downloading Java {major}…",
                "正在下载 Java {major}…"
            ),
            // 正文里那一句：与按钮**不复读**（同一句话在屏幕上出现两次等于没说，复审 r1 nit）。
            JavaGuardDownloadingHint => (
                "This one needs a download; this dialog updates when it finishes",
                "这一步要联网下载，装完这个框会自己更新"
            ),
            JavaGuardDownloadDone => ("Java {major} is ready", "Java {major} 已装好"),
            JavaGuardDownloadFailed => (
                "Java {major} could not be installed ({reason})",
                "Java {major} 没能装上（{reason}）"
            ),
            JavaGuardRetryLaunch => ("Launch now", "现在启动"),
            // 链上全是「跳过」：没有一家发行版提供这个大版本（点名没有意义，只报事实）。
            JavaGuardDownloadNoVendor => (
                "No vendor ships Java {major}",
                "没有一家发行版提供 Java {major}"
            ),
            // 连落点都解析不出来（本地问题，走到任何一家之前就停了）：原文照贴，不翻译。
            JavaGuardDownloadFailedLocal => (
                "The download could not start ({reason})",
                "这次下载没能开始（{reason}）"
            ),
            // 发行版名走目录，**不把内部 id（`temurin`）直接上屏**——与「命令行不印内部 key」
            // 同一条纪律。五家与 `core::game::java::VENDOR_IDS` 一一对应；中文那半带上发行方
            // 的中文名（纯拉丁字母的两行会被 `chinese_catalog_is_actually_chinese` 判成未翻译）。
            JavaVendorTemurin => ("Eclipse Temurin", "Eclipse Temurin 发行版"),
            JavaVendorOracle => ("Oracle JDK", "Oracle JDK（甲骨文）"),
            JavaVendorMicrosoft => ("Microsoft OpenJDK", "Microsoft OpenJDK（微软）"),
            JavaVendorZulu => ("Azul Zulu", "Azul Zulu 发行版"),
            JavaVendorTermux => ("Termux system package", "Termux 系统包"),
            JavaVendorUnknown => ("unknown vendor", "未知发行版"),
        } }
    };
}

pub(crate) use java_install_entries;
