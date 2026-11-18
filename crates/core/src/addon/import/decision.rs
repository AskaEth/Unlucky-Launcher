//! 逐文件冲突决策：`path → Keep / Overwrite`，以及"没收到决策就等于保留"这条硬线。
//!
//! # 三个入口，一个通道
//!
//! | 用户在界面上点的 | 落到这里的形状 |
//! |---|---|
//! | 全部保留 | [`ConflictDecisions::keep_all`]（= [`ConflictDecisions::default`]） |
//! | 全部覆盖 | [`ConflictDecisions::overwrite_all`] |
//! | 逐项 | [`ConflictDecisions::with`] 一条一条挂上去，整批档仍是"保留" |
//!
//! 三者共用一个类型，是因为它们要回答的是同一个问题：「这个路径上的冲突该怎么处置」。
//! 两条独立的落盘规则迟早会打架（一处按整批档、一处按逐项表），所以合并成一处。
//!
//! # 缺省为什么必须保留
//!
//! [`ConflictChoice::default`] 是 `KeepUser`：**表里没有这个路径**、或者调用方根本没设过决策时，
//! 结论是"用户那份文件留着"。这不是保守，是正确性——导入的输入是别人做的整合包，用户的
//! 配置/存档/自己改过的 mod 不能因为"我们没问"就消失。要看"覆盖"只有一个来路：调用方
//! **显式**说了覆盖（整批档或逐项决策）。
//!
//! # 决策只作用于**冲突**
//!
//! 非冲突路径（磁盘上没有、或内容与包内一致）根本不看这张表：没有冲突就没有"保留/覆盖"这个
//! 问题，`merge::decide` 的结论照旧。于是"表里没有这条"永远不会把一份该装的文件漏掉。

use std::collections::BTreeMap;

use super::report::ConflictChoice;

/// 一次导入的逐文件决策表。
///
/// 顺序稳定（`BTreeMap`），所以同一份决策序列化出来的形状是确定的——诊断与测试都能直接比。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictDecisions {
    /// 表里没点名的冲突按它处置。
    default: ConflictChoice,
    /// 逐项决策（实例内相对路径 → 处置）。
    entries: BTreeMap<String, ConflictChoice>,
}

impl Default for ConflictDecisions {
    fn default() -> Self {
        Self {
            // **不许改**：这是"没收到决策不覆盖"的落点。
            default: ConflictChoice::KeepUser,
            entries: BTreeMap::new(),
        }
    }
}

impl ConflictDecisions {
    /// 全部保留（= 缺省）。
    #[must_use]
    pub fn keep_all() -> Self {
        Self::default()
    }

    /// 全部覆盖（用户显式点了「整合包说了算」）。
    #[must_use]
    pub fn overwrite_all() -> Self {
        Self {
            default: ConflictChoice::Overwrite,
            entries: BTreeMap::new(),
        }
    }

    /// 改整批缺省档（逐项决策优先于它）。
    #[must_use]
    pub fn with_default(mut self, choice: ConflictChoice) -> Self {
        self.default = choice;
        self
    }

    /// 挂一条逐项决策（链式）。
    #[must_use]
    pub fn with(mut self, path: impl Into<String>, choice: ConflictChoice) -> Self {
        self.set(path, choice);
        self
    }

    /// 挂一条逐项决策（就地）。
    pub fn set(&mut self, path: impl Into<String>, choice: ConflictChoice) {
        self.entries.insert(path.into(), choice);
    }

    /// 整批缺省档。
    #[must_use]
    pub const fn default_choice(&self) -> ConflictChoice {
        self.default
    }

    /// 这个路径的处置：逐项决策优先，其次整批档（**整批档缺省是保留**）。
    #[must_use]
    pub fn choice_for(&self, path: &str) -> ConflictChoice {
        self.entries.get(path).copied().unwrap_or(self.default)
    }

    /// 这个路径有没有被逐项点名（诊断用：区分"用户选的"与"缺省档"）。
    #[must_use]
    pub fn explicit(&self, path: &str) -> Option<ConflictChoice> {
        self.entries.get(path).copied()
    }

    /// 逐项决策的条数（不含整批缺省档）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 有没有逐项决策。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 逐项决策（按路径排序；进报告/日志用）。
    pub fn iter(&self) -> impl Iterator<Item = (&str, ConflictChoice)> {
        self.entries
            .iter()
            .map(|(path, choice)| (path.as_str(), *choice))
    }

    /// 逐项决策里"覆盖"的条数。
    #[must_use]
    pub fn overwrite_count(&self) -> usize {
        self.entries
            .values()
            .filter(|choice| **choice == ConflictChoice::Overwrite)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_decision_never_overwrites() {
        // 这张表是"没收到决策就覆盖"的唯一防线：默认档与空表的答案都必须是保留。
        assert_eq!(ConflictChoice::default(), ConflictChoice::KeepUser);
        let empty = ConflictDecisions::default();
        assert_eq!(empty.choice_for("config/a.toml"), ConflictChoice::KeepUser);
        assert_eq!(
            ConflictDecisions::keep_all().default_choice(),
            ConflictChoice::KeepUser
        );
        assert!(empty.is_empty());
    }

    #[test]
    fn per_item_wins_over_the_batch_default() {
        let decisions =
            ConflictDecisions::overwrite_all().with("mods/pin.jar", ConflictChoice::KeepUser);
        assert_eq!(
            decisions.choice_for("mods/pin.jar"),
            ConflictChoice::KeepUser
        );
        assert_eq!(
            decisions.choice_for("mods/other.jar"),
            ConflictChoice::Overwrite
        );
        assert_eq!(
            decisions.explicit("mods/pin.jar"),
            Some(ConflictChoice::KeepUser)
        );
        assert_eq!(decisions.explicit("mods/other.jar"), None);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions.overwrite_count(), 0);
    }

    #[test]
    fn set_is_idempotent_per_path() {
        let mut decisions = ConflictDecisions::keep_all();
        decisions.set("a.toml", ConflictChoice::Overwrite);
        decisions.set("a.toml", ConflictChoice::KeepUser);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions.choice_for("a.toml"), ConflictChoice::KeepUser);
        assert_eq!(decisions.overwrite_count(), 0);
        let paths: Vec<&str> = decisions.iter().map(|(path, _)| path).collect();
        assert_eq!(paths, vec!["a.toml"]);
    }
}
