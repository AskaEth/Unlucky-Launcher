//! 当前实例对 Java 的**要求**与「某个大版本够不够」的判定（M197 从 `state.rs` 拆出）。
//!
//! M175 只回答了「满不满足」（一个 `bool`），于是**方向说不清**：1.12.2 只能吃 Java 8，
//! 用户手里是 Java 21 时旧文案写的是「低于要求：当前实例需要 Java 8 及以上」——那句话自相矛盾
//! （21 当然 ≥ 8）。M197 的第三条任务（「不许静默失败，要说清这个版本需要 Java X，当前是 Y」）
//! 要求两个方向各说各的，所以这里改成带方向的 [`Fit`]。
//!
//! 判据本身来自 `core::game::java::mapping`（静态表 + 版本 JSON 的 `javaVersion`），
//! 本文件只是把它翻成页面的三态；**不复制**那张表。

/// 当前实例对 Java 的版本要求（app 层算好塞进来）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// 实例名（「为什么选它」那句话里要指名道姓）
    pub instance: String,
    /// 实例绑的版本 id（`1.20.1`）
    pub version: String,
    /// 最低主版本
    pub minimum: u32,
    /// 最高主版本（老版本才有；`None` = 不封顶）
    pub maximum: Option<u32>,
}

impl Requirement {
    /// 某个大版本满不满足。
    #[must_use]
    pub fn satisfied_by(&self, major: u32) -> bool {
        major >= self.minimum && self.maximum.is_none_or(|max| major <= max)
    }

    /// 某个大版本**怎么不满足**（满足时给 [`Fit::Ok`]）。
    ///
    /// 两个方向的判据与 `mapping::check` 一致（下限来自版本清单 / 静态表，上限只对老版本存在）。
    #[must_use]
    pub fn fit(&self, major: u32) -> Fit {
        if major < self.minimum {
            return Fit::TooOld {
                required: self.minimum,
                actual: major,
            };
        }
        match self.maximum {
            Some(maximum) if major > maximum => Fit::TooNew {
                maximum,
                actual: major,
            },
            _ => Fit::Ok,
        }
    }

    /// 要求的人读形态（`Java 21+` / `Java 8`）——**不含**"当前是什么"，那是 [`Fit`] 的活。
    #[must_use]
    pub fn describe(&self) -> String {
        match self.maximum {
            Some(maximum) if maximum == self.minimum => format!("Java {}", self.minimum),
            Some(maximum) => format!("Java {}–{}", self.minimum, maximum),
            None => format!("Java {}+", self.minimum),
        }
    }
}

/// 「这个大版本够不够用」的三态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// 满足要求
    Ok,
    /// 太低（当前实例要更老的之上的版本）
    TooOld {
        /// 需要的最低主版本
        required: u32,
        /// 实际主版本
        actual: u32,
    },
    /// 太高（老加载器吃不了新的 class 文件）
    TooNew {
        /// 允许的最高主版本
        maximum: u32,
        /// 实际主版本
        actual: u32,
    },
}

impl Fit {
    /// 是不是「不满足」。
    #[must_use]
    pub const fn is_mismatch(self) -> bool {
        !matches!(self, Self::Ok)
    }
}

/// 生效内存来自哪一层。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layer {
    /// 全局 `java.default_memory_mb`
    Global,
    /// 某个实例的 `launch.memoryMb` 覆盖了全局
    Instance {
        /// 实例名
        instance: String,
        /// 实例层的值
        mb: u32,
    },
}

/// 上下文（app 层每次 hydrate 重算）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    /// 当前实例的版本要求（没有实例 ⇒ `None`）
    pub required: Option<Requirement>,
    /// 当前生效内存来自哪一层
    pub layer: Option<Layer>,
    /// 当前生效内存（三级回落之后的数值）
    pub effective_memory_mb: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::{Fit, Requirement};

    fn legacy() -> Requirement {
        Requirement {
            instance: "老服".to_string(),
            version: "1.12.2".to_string(),
            minimum: 8,
            maximum: Some(8),
        }
    }

    fn modern() -> Requirement {
        Requirement {
            instance: "生存服".to_string(),
            version: "1.20.5".to_string(),
            minimum: 21,
            maximum: None,
        }
    }

    /// 两个方向各说各的：Java 21 对 1.12.2 是**太高**，不是「低于要求」。
    ///
    /// M175 的 `satisfied()` 只给布尔值，文案于是必然写错一个方向（复审实测到的现象）。
    /// 变异证明：把 `fit` 里的 `maximum` 分支删掉（一律 `Fit::Ok`）⇒ 本用例红。
    #[test]
    fn the_direction_of_the_mismatch_is_reported() {
        assert_eq!(
            legacy().fit(21),
            Fit::TooNew {
                maximum: 8,
                actual: 21
            }
        );
        assert_eq!(
            modern().fit(17),
            Fit::TooOld {
                required: 21,
                actual: 17
            }
        );
        assert_eq!(modern().fit(21), Fit::Ok);
        assert_eq!(modern().fit(25), Fit::Ok, "不封顶时更高的版本也满足");
        assert_eq!(legacy().fit(8), Fit::Ok);
        assert!(legacy().fit(21).is_mismatch());
        assert!(!modern().fit(21).is_mismatch());
    }

    /// 要求的人读形态（给下载项 / 运行时行共用）。
    #[test]
    fn the_requirement_reads_as_a_range() {
        assert_eq!(legacy().describe(), "Java 8");
        assert_eq!(modern().describe(), "Java 21+");
        assert_eq!(
            Requirement {
                minimum: 8,
                maximum: Some(16),
                ..modern()
            }
            .describe(),
            "Java 8–16",
            "有上限时区间两端都要写出来（1.17 的 8–16 就是一个真实区间）"
        );
    }
}
