//! `wrap` 的断言：软断点插在哪、插完还能不能还原。
//!
//! 口径与 `layout/tests.rs` 一致：只钉**纯函数**（断点位置 / 原文可还原），
//! 「渲染器是不是真在那里折」由截图与报告里的前后对照图负责。

use super::*;

/// 视觉复审 ③ 的原话样本：这两个被劈开的词必须重新可断。
#[test]
fn identifiers_break_after_their_separators() {
    let summary =
        "findings=integrity.unsatisfied_link:high(85),graphics.gl_operation_failure:medium(70)";
    let broken = breakable(summary);

    // 抓得住：断点漏掉 `_` / `.` / `:`（那就退化成按字形硬断，`graphics.gl_o` + `peration_failure` 复现）；
    // 抓不住：渲染器最终在哪一列折行（那要量图，见报告）。
    for seam in [
        "integrity.\u{200b}",
        "unsatisfied_\u{200b}",
        "link:\u{200b}",
        "graphics.\u{200b}",
        "gl_\u{200b}",
        "operation_\u{200b}",
        "failure:\u{200b}",
    ] {
        assert!(broken.contains(seam), "缺断点 {seam:?}：{broken}");
    }
}

/// 路径样本：`logs/2026-11-02/client.txt` 在 `/` 与 `.` 后能断，日期里的 `-` 不许断。
#[test]
fn paths_break_after_slashes_but_not_inside_dates() {
    let broken = breakable("logs/2026-11-02/client.txt");
    assert!(broken.contains("logs/\u{200b}"), "{broken}");
    assert!(broken.contains("client.\u{200b}txt"), "{broken}");
    // 日期整体不拆：`-` 不在断点表里，视觉复审没要求、拆开反而难读。
    assert!(broken.contains("2026-11-02"), "{broken}");

    // Windows 路径：反斜杠同样是断点（日志里 `AppData\Roaming\...\dev-test\natives\...`）
    let win =
        breakable("C:\\Users\\Player\\AppData\\Roaming\\.unlucky\\instances\\dev-test\\natives");
    assert!(win.contains("Player\\\u{200b}"), "{win}");
    assert!(win.contains("natives"), "{win}");
}

/// 不该插的地方一个都不插：结尾、空白之后、连续分隔符。
#[test]
fn no_dangling_or_doubled_breaks() {
    assert_eq!(breakable(""), "");
    assert_eq!(breakable("plain text"), "plain text");
    assert_eq!(breakable("trailing."), "trailing.", "结尾不插");
    assert_eq!(
        breakable("a. b"),
        "a. b",
        "分隔符后面是空白：Wrapping 本来就能在空白处折"
    );
    assert_eq!(
        breakable("a..b"),
        "a..\u{200b}b",
        "连续分隔符只在最后一处断"
    );
    assert_eq!(breakable("a:/b"), "a:/\u{200b}b");

    // 抓得住：有人在每个字符后面都插（那就等于按字形断，本 mission 的修法失效）。
    assert!(
        !breakable("abcdef").contains(SOFT_BREAK),
        "没有任何分隔符的串不该出现断点"
    );
}

/// 只改显示：`strip` 还原后与原文逐字符相同（剪贴板/日志原文不许被污染）。
#[test]
fn breaking_is_reversible_and_idempotent() {
    for text in [
        "graphics.gl_operation_failure:medium(70)",
        "[19:40:12] [main/ERROR] 启动失败: exit code 1",
        "at com.mojang.blaze3d.platform.GLX._initGlfw(GLX.java:64)",
    ] {
        let broken = breakable(text);
        assert_eq!(strip(&broken), text, "还原失败：{broken}");
        assert_eq!(breakable(&broken), broken, "幂等：再跑一遍不许再插");
    }
}

/// **已知限界**（r1 复审 P2）：`strip` 删的是**所有** U+200B，原文自带的也会被删。
///
/// 这条不修（产品侧从不调用 `strip`，剪贴板/模型拿的是未改写的原文），但要把行为钉住：
/// 将来谁想拿 `strip` 当通用还原器，这里会告诉他为什么不行。
#[test]
fn strip_is_not_a_general_unbreaker() {
    let original_with_zwsp = "a\u{200b}b";
    assert_eq!(
        breakable(original_with_zwsp),
        original_with_zwsp,
        "没有分隔符就不插"
    );
    assert_eq!(
        strip(original_with_zwsp),
        "ab",
        "strip 会连同原文自带的 U+200B 一起删（已知限界，不是 bug 修复点）"
    );
}
