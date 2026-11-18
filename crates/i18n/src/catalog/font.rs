//! 中文字体文案（D28）：中文环境缺字体时的下载提示 / 失败回退。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! font_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 中文字体（D28：中文环境缺字体时的下载提示 / 失败回退）
            FontCjkNeedsDownload => (
                "No Chinese font found on this system. Download an open-source (OFL) Chinese font?",
                "系统里没找到中文字体。要下载一套开源（OFL）中文字体吗？"
            ),
            FontCjkDownloading => ("Downloading Chinese font… {percent}%", "正在下载中文字体… {percent}%"),
            FontCjkDownloadFailed => (
                "Chinese font download failed; the interface stays in English.",
                "中文字体下载失败，界面暂时保持英文。"
            ),
            FontCjkReady => ("Chinese font ready: {family}", "中文字体已就绪：{family}"),
        } }
    };
}

pub(crate) use font_entries;
