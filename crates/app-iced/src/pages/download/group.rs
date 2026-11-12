//! 主版本分组：把一串版本 id 归成「大版本」桶（第二级侧栏的数据源）。
//!
//! ## 为什么不能按第一个 `.` 切
//!
//! 真实的版本清单里同时存在**四种形态**（拿真样本说，不用「第几个点」这种相对说法）：
//!
//! | 样本 | 是什么 | `split('.')` 会得到什么 | 正确的主版本 |
//! |---|---|---|---|
//! | `1.21.4` | 正式版 | `["1", "21", "4"]` | `1.21` |
//! | `1.21.9-pre3` | 预发布（正式版的兄弟） | `["1", "21", "9-pre3"]` | `1.21` |
//! | `24w14a` | 周更快照 | `["24w14a"]`（一个都没有） | `24w`（周更的年桶） |
//! | `rd-132211` / `a1.2.6` | 远古版（Classic / Alpha） | `["rd-132211"]` / `["a1", "2", "6"]` | **没有主版本** |
//!
//! 「按第一个 `.` 切」在 `1.21.4` 上给出 `1`，把 1.7 到 1.21 的全部正式版塞进一个桶；
//! 在 `a1.2.6` 上给出 `a1`，凭空造出一个「主版本」——远古版根本没有 major.minor 这层概念。
//!
//! ## 规则（三条，按顺序命中即返回）
//!
//! 1. **带点的数值前缀**（正式版 / 预发布）：id 在第一个 `-` 之前的部分，前两段都是纯数字
//!    ⇒ 主版本 = `第一段.第二段`。`1.21.9-pre3` 先切掉 `-pre3` 再取 `1.21`。
//! 2. **周更快照**：id 形如 `24w14a`（两位年 + `w` + 两位周 + 一个修订字母）⇒ 主版本 = `24w`。
//!    周更版本每年几十个，年桶是它的「大版本」等价物；不按周切是因为一周通常只有 1 个版本，
//!    按周分组等于每个桶一个版本（侧栏退化成重复的列表）。
//! 3. **其余**：返回 [`None`]（没有主版本概念）。远古版（`rd-132211` / `a1.2.6` / `c0.30_01c`）
//!    走这一条 ⇒ 第二级侧栏只有一个「全部」项。
//!
//! 判据是**形态**（有没有 `-` 后缀、前两段是不是纯数字、有没有 `NNwNNx` 的形状），
//! 不是「版本号大小」，所以新增一个 `1.22.0` 或 `25w01a` 不需要改规则。

use unlucky_presenter::dto::VersionKind;

/// 主版本桶的一个标签（`1.21` / `24w`）；没有主版本概念时是 [`None`]。
pub type MajorKey = String;

/// 这个版本属于哪个主版本桶（规则见模块文档）。
///
/// 远古版**直接**返回 [`None`]：它们的 id 形态（`a1.2.6`）与正式版同族，靠形态规则会误判，
/// 所以先按 `kind` 分流——「有没有主版本」是**版本类型**的属性，不是 id 字符串的属性。
#[must_use]
pub fn key(id: &str, kind: VersionKind) -> Option<MajorKey> {
    if kind.is_ancient() {
        return None;
    }

    // 规则 1：切掉 `-pre3` / `-rc1` 这类后缀，再取前两段数值
    let head = id.split('-').next().unwrap_or(id);
    let mut parts = head.split('.');
    if let (Some(major), Some(minor)) = (parts.next(), parts.next()) {
        if is_number(major) && is_number(minor) {
            return Some(format!("{major}.{minor}"));
        }
    }

    // 规则 2：周更快照 `24w14a` → `24w`
    if let Some((year, tail)) = head.split_once('w') {
        let week_like = tail.chars().next().is_some_and(|ch| ch.is_ascii_digit());
        if year.len() == 2 && is_number(year) && week_like {
            return Some(format!("{year}w"));
        }
    }

    // 规则 3：没有主版本概念
    None
}

/// 纯数字（空串不算）。
fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// 两条主版本标签的**版本序**（降序用）：`1.21 > 1.9`、`24w > 14w`，数字按数值比、字母按字节比。
///
/// 为什么不是字符串序：`"1.9" > "1.21"` 在字典序里成立（`9 > 2`），而版本序里 1.21 更新。
/// 逐段比：连续数字读成整数、其余字符逐字节比。
#[must_use]
pub fn compare(left: &str, right: &str) -> std::cmp::Ordering {
    let mut left_rest = left;
    let mut right_rest = right;

    loop {
        match (left_rest.chars().next(), right_rest.chars().next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(left_char), Some(right_char)) => {
                if left_char.is_ascii_digit() && right_char.is_ascii_digit() {
                    let (left_number, left_tail) = take_number(left_rest);
                    let (right_number, right_tail) = take_number(right_rest);
                    match left_number.cmp(&right_number) {
                        std::cmp::Ordering::Equal => {
                            left_rest = left_tail;
                            right_rest = right_tail;
                        }
                        other => return other,
                    }
                } else if left_char == right_char {
                    left_rest = &left_rest[left_char.len_utf8()..];
                    right_rest = &right_rest[right_char.len_utf8()..];
                } else {
                    return left_char.cmp(&right_char);
                }
            }
        }
    }
}

/// 取出开头的连续数字（返回数值与剩余部分）。
fn take_number(text: &str) -> (u64, &str) {
    let end = text
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(text.len());
    (text[..end].parse().unwrap_or(0), &text[end..])
}

/// 一个主版本桶（第二级侧栏的一行）：桶标签 + 桶里有多少个版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket {
    /// 桶标签（`1.21` / `24w`）；[`None`] = 这一组版本没有主版本概念（远古版）。
    pub key: Option<MajorKey>,
    /// 桶里的版本数（列表列与命中数都读它）。
    pub count: usize,
}

/// 把版本行归成桶：**按版本序降序**（`1.21` 在 `1.20` 前面），无主版本概念的桶排最后。
///
/// 只做聚合与排序，不决定「当前选中哪个桶」——那是页面状态，住在 `pages/Download` 里。
#[must_use]
pub fn buckets(rows: &[super::model::VersionRow]) -> Vec<Bucket> {
    let mut grouped: Vec<Bucket> = Vec::new();
    for row in rows {
        let key = row.major();
        match grouped.iter_mut().find(|bucket| bucket.key == key) {
            Some(bucket) => bucket.count += 1,
            None => grouped.push(Bucket { key, count: 1 }),
        }
    }

    grouped.sort_by(|left, right| match (&left.key, &right.key) {
        (Some(a), Some(b)) => compare(b, a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    grouped
}

/// 第二级侧栏此刻**真正生效**的桶（P2-1 的口径）。
///
/// 搜索时一律放宽到「全部」（[`None`]）：命中数 `hits` 是**跨桶**读数，若列表还按桶过滤，
/// 就会出现「N matches + 0 versions + 没有版本」的自相矛盾一屏（复审 P2-1）。
/// 放宽之后「列表行数 = 当前分类里的命中数」，侧栏也不会有「选中项不在桶列表里」的幽灵态。
/// `selected_major` 一直留在模型里，只是搜索期间不生效——清空搜索后原来的桶自动回来。
#[must_use]
pub fn active_bucket(model: &super::model::Download) -> Option<&str> {
    if model.has_query() {
        None
    } else {
        model.selected_major.as_deref()
    }
}

/// 第二级侧栏里这一项该不该点亮（`None` = 「全部版本」那一项）。
#[must_use]
pub fn bucket_selected(model: &super::model::Download, key: Option<&str>) -> bool {
    active_bucket(model) == key
}

#[cfg(test)]
mod tests;
