//! 离线档案的名字契约（**UI 与 host 共用的同一个判据**）。
//!
//! 放在 `dto` 而不是 host 里：UI 用它决定「创建档案」按钮亮不亮，host 用它做写盘前校验。
//! 两处各写一遍，迟早会漂成「按钮亮着但必然失败」——那正是复审 r1 P2-2 的一半。
//! core 侧还有一道同名规则做兜底（`auth::offline::offline_meta`），这里只管界面这条。

/// 3–16 位 ascii 字母 / 数字 / 下划线。
#[must_use]
pub fn valid_offline_name(name: &str) -> bool {
    let len = name.chars().count();
    (3..=16).contains(&len) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_is_three_to_sixteen_ascii_word_chars() {
        assert!(valid_offline_name("Steve"));
        assert!(valid_offline_name("a_1"));
        assert!(valid_offline_name("abcdefghijklmnop"), "16 位是上限");
        assert!(!valid_offline_name("ab"), "太短");
        assert!(!valid_offline_name("abcdefghijklmnopq"), "太长");
        assert!(!valid_offline_name("has space"));
        assert!(!valid_offline_name("中文名字"), "非 ascii");
        assert!(!valid_offline_name(""));
    }
}
