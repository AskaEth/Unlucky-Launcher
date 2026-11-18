//! 版本串比较与版本范围判定（M312 · MODDUP）。
//!
//! # 为什么自己写一份
//!
//! 仓里已有的两份都不能用：`game::java::oracle::listing::version_key` 只认纯数字段（Java 清单），
//! `addon::loader::liteloader::versions::cmp_versions` 是那个模块的私有函数（拿不出来，动它超出
//! M312 的改动范围）。所以这里是第三份——但**只需要一个方向**：mod 版本串谁新谁旧。
//!
//! # 只回答一个问题：谁更新
//!
//! 分段比较（`1.0.2` → `[1, 0, 2]`、`0.5.11+build.3` → `[0, 5, 11, "build", 3]`）：
//! 数字段按数值、字母段按字典序、数字段**大于**字母段（发布版比 `-beta` 新）；缺的段按 0 补齐。
//! 这不是 semver，也不假装是：`v` 前缀、四位版本、`1.0.0-rc1` 这些都能排出个先后，
//! 但**没有** 预发布的完整语义。判定只在「同一份 mod 的两个版本」上用，这个精度够。
//!
//! # 版本范围：只承认能确认的写法
//!
//! [`satisfies`] 返回三态，只有 `Yes` 表示"确认满足"：
//!
//! | 写法 | 判定 |
//! |---|---|
//! | 没写 / 空白 / `*` / `+` | `Yes`（没有版本要求） |
//! | `>=1.2` `>1.2` `<=1.2` `<1.2` `=1.2` `==1.2` | 用 [`cmp_version`] 判（能确认） |
//! | 裸版本 `1.2.3` | `Unknown`——Fabric 当精确匹配、Maven（Forge）当软下界，**两家语义不同** |
//! | Maven 区间 `[1.0,2.0)` `(,1.0]` / 逗号列表 | `Unknown`——开闭语义我们没实现 |
//! | 装了但版本读不出 | `Unknown` |
//!
//! `Unknown` 在报告里落到「无法判定」那一栏，**不是**「缺失」——误报会让用户去删一个没问题的 mod
//! （`mods.toml` 里最常见的 `versionRange="[1.20.1,1.21)"` 正是这一档）。

use std::cmp::Ordering;

/// 版本范围的判定结果（三态：`Unknown` 是"判不了"，不是"不满足"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Satisfaction {
    /// 确认满足。
    Yes,
    /// 确认**不**满足（能算出来才敢这么说）。
    No,
    /// 判不了（写法不认识 / 装的那份没版本）。
    Unknown,
}

/// 比较两个版本串（`Greater` = `left` 更新）。
#[must_use]
pub fn cmp_version(left: &str, right: &str) -> Ordering {
    let left = segments(trim_prefix(left));
    let right = segments(trim_prefix(right));
    for index in 0..left.len().max(right.len()) {
        let order = compare(
            left.get(index).copied().unwrap_or(Segment::Empty),
            right.get(index).copied().unwrap_or(Segment::Empty),
        );
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}

/// 判一条版本范围（`installed` = 盘上那一份的版本）。
#[must_use]
pub fn satisfies(installed: Option<&str>, range: Option<&str>) -> Satisfaction {
    let constrained = range
        .map(str::trim)
        .filter(|range| !range.is_empty() && !matches!(*range, "*" | "+" | "x" | "X"));
    let Some(range) = constrained else {
        return Satisfaction::Yes;
    };
    let Some(installed) = installed.map(str::trim).filter(|it| !it.is_empty()) else {
        return Satisfaction::Unknown;
    };
    // 组合写法（逗号列表 `>=1.0,<2.0` / Maven 区间 `[1.0,2.0)`）要解开闭语义与"且"关系，
    // 我们没实现 ⇒ 判不了（别把 `>=1.0,<2.0` 读成只有一个下界）。
    if range.contains(',') || range.contains('[') || range.contains('(') {
        return Satisfaction::Unknown;
    }
    // 前缀运算符（长的先比，否则 `>=` 会被 `>` 抢走）。
    let Some((operator, bound)) = ["<=", ">=", "==", "<", ">", "="]
        .iter()
        .find_map(|operator| {
            range
                .strip_prefix(operator)
                .map(|rest| (*operator, rest.trim()))
        })
    else {
        // 裸版本 / Maven 区间 / 逗号列表：两种加载器的语义不同，不猜。
        return Satisfaction::Unknown;
    };
    if bound.is_empty() {
        return Satisfaction::Unknown;
    }
    let order = cmp_version(installed, bound);
    let ok = match operator {
        ">=" => order.is_ge(),
        ">" => order.is_gt(),
        "<=" => order.is_le(),
        "<" => order.is_lt(),
        _ => order.is_eq(),
    };
    if ok {
        Satisfaction::Yes
    } else {
        Satisfaction::No
    }
}

/// 去掉 `v` / `V` 前缀（`v1.2` 与 `1.2` 是同一个版本）。
fn trim_prefix(version: &str) -> &str {
    let trimmed = version.trim();
    match trimmed.strip_prefix(['v', 'V']) {
        Some(rest) if rest.starts_with(|ch: char| ch.is_ascii_digit()) => rest,
        _ => trimmed,
    }
}

/// 版本串的一个段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment<'a> {
    /// 数字段（`1.0.2` 里的三个）。
    Number(u128),
    /// 字母段（`-beta` / `+build` 里的词，原样保留了大小写）。
    Word(&'a str),
    /// 这一侧已经用完了（缺段按 0 补齐的"0"）。
    Empty,
}

fn compare(left: Segment<'_>, right: Segment<'_>) -> Ordering {
    match (left, right) {
        (Segment::Number(left), Segment::Number(right)) => left.cmp(&right),
        (Segment::Word(left), Segment::Word(right)) => left.cmp(right),
        // 发布版比预发布新：`1.0.1` > `1.0-beta`。
        (Segment::Number(_), Segment::Word(_)) => Ordering::Greater,
        (Segment::Word(_), Segment::Number(_)) => Ordering::Less,
        (Segment::Empty, Segment::Empty) => Ordering::Equal,
        (Segment::Empty, Segment::Number(0)) | (Segment::Number(0), Segment::Empty) => {
            Ordering::Equal
        }
        (Segment::Empty, Segment::Number(_)) => Ordering::Less,
        (Segment::Number(_), Segment::Empty) => Ordering::Greater,
        (Segment::Empty, Segment::Word(_)) => Ordering::Less,
        (Segment::Word(_), Segment::Empty) => Ordering::Greater,
    }
}

/// 按「连续数字 / 连续字母」切段，其余字符（`.` `-` `+` `_` …）只是分隔符。
fn segments(text: &str) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    let mut current: Option<(usize, usize, bool)> = None;
    for (index, ch) in text.char_indices() {
        let kind = if ch.is_ascii_digit() {
            Some(true)
        } else if ch.is_ascii_alphabetic() {
            Some(false)
        } else {
            None
        };
        match (current, kind) {
            (Some((start, _, digits)), Some(kind)) if kind == digits => {
                current = Some((start, index + ch.len_utf8(), digits));
            }
            (Some((start, end, digits)), next) => {
                push(&mut out, text, start, end, digits);
                current = next.map(|digits| (index, index + ch.len_utf8(), digits));
            }
            (None, Some(digits)) => current = Some((index, index + ch.len_utf8(), digits)),
            (None, None) => {}
        }
    }
    if let Some((start, end, digits)) = current {
        push(&mut out, text, start, end, digits);
    }
    out
}

fn push<'a>(out: &mut Vec<Segment<'a>>, text: &'a str, start: usize, end: usize, digits: bool) {
    let slice = &text[start..end];
    if digits {
        // 超长数字段（恶意/手误）封顶到 `u128::MAX`：比较结论仍然稳定，不会 panic。
        out.push(Segment::Number(slice.parse().unwrap_or(u128::MAX)));
    } else {
        out.push(Segment::Word(slice));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 谁更新：数字段按数值（`0.5.11` > `0.5.8`，不是字典序）。
    #[test]
    fn newer_versions_win_by_number() {
        assert_eq!(cmp_version("0.5.11", "0.5.8"), Ordering::Greater);
        assert_eq!(cmp_version("1.20.1", "1.20.10"), Ordering::Less);
        assert_eq!(cmp_version("2", "2.0.0"), Ordering::Equal);
        assert_eq!(cmp_version("v1.2", "1.2.0"), Ordering::Equal);
        // 发布版比预发布新。
        assert_eq!(cmp_version("1.0.1", "1.0-beta"), Ordering::Greater);
        assert_eq!(cmp_version("15.2.0.27", "15.2.0.26"), Ordering::Greater);
    }

    /// 能确认的写法给 Yes/No；两家语义不同的写法一律 `Unknown`（宁可判不了，不误报）。
    #[test]
    fn only_confirmable_ranges_are_judged() {
        assert_eq!(
            satisfies(Some("0.15.1"), Some(">=0.14.0")),
            Satisfaction::Yes
        );
        assert_eq!(
            satisfies(Some("0.13.0"), Some(">=0.14.0")),
            Satisfaction::No
        );
        assert_eq!(satisfies(Some("0.15.1"), Some("<1.0")), Satisfaction::Yes);
        assert_eq!(satisfies(Some("1.0"), Some("=1.0")), Satisfaction::Yes);
        assert_eq!(satisfies(Some("1.0"), Some("==1.0")), Satisfaction::Yes);

        // 没有版本要求 = 满足。
        for range in [None, Some(""), Some("*"), Some("  ")] {
            assert_eq!(
                satisfies(Some("1.0"), range),
                Satisfaction::Yes,
                "{range:?}"
            );
            assert_eq!(satisfies(None, range), Satisfaction::Yes, "{range:?}");
        }

        // 判不了的写法：裸版本（Maven 与 Fabric 语义不同）、Maven 区间、逗号列表。
        for range in ["1.2.3", "[1.0,2.0)", "(,1.0]", ">=1.0,<2.0"] {
            assert_eq!(
                satisfies(Some("9.9"), Some(range)),
                Satisfaction::Unknown,
                "{range}"
            );
        }
        // 装了但版本读不出 ⇒ 判不了。
        assert_eq!(satisfies(None, Some(">=1.0")), Satisfaction::Unknown);
    }
}
