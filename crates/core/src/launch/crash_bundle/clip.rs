//! 超限文件的**文本流**截断（头 + 尾），切点落在字符/行边界上。
//!
//! ## 为什么截断必须发生在「文本流」里，而不是字节上
//!
//! 上一版在**字节**上切两刀（头 2 MiB / 尾 2 MiB），切完才试 `String::from_utf8`
//! 决定要不要脱敏。中文与 emoji 是 3~4 字节，只要切点落在字中间，`from_utf8` 就失败，
//! 于是**整份文件退回「原样字节」分支、一条脱敏规则都没跑**（r2 复审 P2-1 抓到的隐私旁路：
//! 长会话的 `latest.log` 与会话 JSONL 正好是最容易超大、也最该脱敏的两个文件；
//! 中文 Windows 用户名让家目录串本身就是 CJK，切在它附近必然命中）。
//!
//! 现在这条通路只有一种形态：**解码 → 脱敏 → 按字符边界截断**。没有「二进制原样」这条分支，
//! 也就没有旁路。
//!
//! ## 为什么先脱敏再截断（而不是先截断再脱敏）
//!
//! 顺序反过来会在切点留下**半截 secret**：被切开的前缀不再匹配任何规则，于是以原文形态留在
//! 包里。先脱敏就没有这个问题——整串已经变成占位符，截断只能切占位符。
//!
//! 代价：要先把整份文本过一遍脱敏器。导出是低频人工动作、上限 4 MiB，代价可以忽略。

/// 在**字符边界**上截断，保留头部与尾部，中间插一行说明。
///
/// 返回 `None` 表示不需要截（预算足够或预算为 0）。
/// 返回值里的 `String` 已按预算裁好；调用方负责把 `truncated` 记进清单。
pub(super) fn clip_text(text: &str, budget: u64) -> Option<String> {
    let budget = usize::try_from(budget).unwrap_or(usize::MAX);
    if budget == 0 || text.len() <= budget {
        return None;
    }

    let head_budget = usize::try_from(super::HEAD_BYTES)
        .unwrap_or(usize::MAX)
        .min(budget / 2);
    let tail_budget = budget - head_budget;

    let head_end = snap_back(text, head_budget);
    let tail_start = snap_forward(text, text.len() - tail_budget);
    if tail_start <= head_end {
        return None;
    }

    let dropped = tail_start - head_end;
    Some(format!(
        "{}\n<truncated {dropped} bytes>\n{}",
        &text[..head_end],
        &text[tail_start..]
    ))
}

/// 从 `index` 向左找最近的**行首**，找不到就在字符边界上回退。
///
/// 行边界只为可读性：切在一条日志行中间，读的人会以为那份日志本身是坏的。
fn snap_back(text: &str, index: usize) -> usize {
    let index = floor_char(text, index);
    let window = floor_char(text, index.saturating_sub(200));
    match text[window..index].rfind('\n') {
        Some(newline) => window + newline + 1,
        None => index,
    }
}

/// 从 `index` 向右找最近的**行首**，找不到就在字符边界上前进。
fn snap_forward(text: &str, index: usize) -> usize {
    let index = ceil_char(text, index);
    let window = ceil_char(text, (index + 200).min(text.len()));
    match text[index..window].find('\n') {
        Some(newline) => index + newline + 1,
        None => index,
    }
}

/// 向左回退到最近的字符边界（`str::floor_char_boundary` 仍是 nightly）。
fn floor_char(text: &str, index: usize) -> usize {
    let mut at = index.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// 向右前进到最近的字符边界。
fn ceil_char(text: &str, index: usize) -> usize {
    let mut at = index.min(text.len());
    while at < text.len() && !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::{clip_text, snap_back, snap_forward};
    use crate::launch::crash_bundle::MAX_ENTRY_BYTES;

    /// 切点必须落在字符边界上（否则下游 `str` 切片会直接 panic）。
    #[test]
    fn cuts_land_on_char_boundaries() {
        // 每个 CJK 字 3 字节：把边界刻意放在字的中间
        let text = "中".repeat(2000);
        let clipped = clip_text(&text, 3 * 500 + 1).expect("应当截断");
        assert!(clipped.contains("<truncated "), "{clipped:.80}");
        assert!(!clipped.contains('\u{fffd}'), "不许出现替换字符");
        // 两段都是合法 UTF-8 边界 ⇒ 能再按 `str` 切
        let head = clipped.split("<truncated ").next().unwrap_or_default();
        assert!(head.chars().all(|ch| ch == '中' || ch == '\n'), "{head:?}");
    }

    #[test]
    fn head_and_tail_are_both_kept_and_snapped_to_lines() {
        let mut text = String::new();
        while text.len() < usize::try_from(MAX_ENTRY_BYTES).expect("常数") + 128 {
            text.push_str("2026-11-08 line with some words\n");
        }
        text.push_str("{\"kind\":\"exit\"}\n");
        let clipped = clip_text(&text, MAX_ENTRY_BYTES).expect("应当截断");
        assert!(clipped.starts_with("2026-11-08 line"), "头部丢了");
        assert!(clipped.ends_with("{\"kind\":\"exit\"}\n"), "尾部丢了");
        assert!(
            clipped.len() as u64 <= MAX_ENTRY_BYTES + 64,
            "截断后不该超出预算（{} > {}）",
            clipped.len(),
            MAX_ENTRY_BYTES
        );
    }

    #[test]
    fn small_files_are_not_touched() {
        assert!(clip_text("short", 1024).is_none());
        assert!(clip_text("short", 0).is_none());
    }

    #[test]
    fn snapping_prefers_line_bounds() {
        let text = "aaaa\nbbbb\ncccc";
        assert_eq!(snap_back(text, 7), 5, "应回退到「bbbb」行首");
        assert_eq!(snap_forward(text, 5), 10, "应前进到「cccc」行首");
    }
}
