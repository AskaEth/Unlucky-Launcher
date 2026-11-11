//! Java 与目标版本**不符**的判定与文案（从 `mapping` 拆出：那边只管版本表与清单推导）。
//!
//! 两种形态（D13 引入，D4 起参与启动决策）：
//!
//! - [`JavaMismatch::TooOld`]：新版本配了太老的 Java（1.20.5+ 需要 21，手里只有 17）；
//! - [`JavaMismatch::TooNew`]：老版本配了新 Java（1.12.2 只能用 8，托管里只有 21/25）。
//!
//! 这里只产出数据与人读文案；**拦不拦**由调用方决定（CLI 见
//! [`crate::game::java::select::decide`]：不符且用户没显式指定 Java 就拦住）。

use super::mapping::JavaRequirement;

/// 不符的两种形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaMismatch {
    /// Java 太低。
    TooOld {
        /// 需要的最低主版本。
        required: u32,
        /// 实际主版本。
        actual: u32,
    },
    /// Java 太高（老版本只能用 8）。
    TooNew {
        /// 允许的最高主版本。
        maximum: u32,
        /// 实际主版本。
        actual: u32,
    },
}

impl JavaMismatch {
    /// 稳定字符串 id（`--json` 的 `mismatch.code`）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::TooOld { .. } => "java_too_old",
            Self::TooNew { .. } => "java_too_new",
        }
    }

    /// 人读说明（GUI 弹窗 P1 后直接复用这句）。
    #[must_use]
    pub fn message(self) -> String {
        match self {
            Self::TooOld { required, actual } => format!(
                "该版本需要 Java {required} 及以上，当前用的是 Java {actual}（大概率起不来）"
            ),
            Self::TooNew { maximum, actual } => format!(
                "该版本只能用 Java {maximum} 及以下，当前用的是 Java {actual}（老加载器吃不了新 class 文件）"
            ),
        }
    }

    /// 一句**可操作**的建议（D4：拦下来之后必须告诉用户怎么继续）。
    ///
    /// 两个出口：装一个对版本的 JDK，或用 `--java` 明确指定要试的那个（显式指定不再被拦）。
    #[must_use]
    pub fn advice(self) -> String {
        let wanted = match self {
            Self::TooOld { required, .. } => required,
            Self::TooNew { maximum, .. } => maximum,
        };
        format!(
            "装一个 Java {wanted}（`java install {wanted}`），或用 `--java <路径>` 指定要试的那个"
        )
    }

    /// 实际主版本号。
    #[must_use]
    pub const fn actual(self) -> u32 {
        match self {
            Self::TooOld { actual, .. } | Self::TooNew { actual, .. } => actual,
        }
    }
}

/// 判需求与实际是否相符（`None` = 相符）。
#[must_use]
pub fn check(actual: u32, required: &JavaRequirement) -> Option<JavaMismatch> {
    if actual < required.minimum {
        return Some(JavaMismatch::TooOld {
            required: required.minimum,
            actual,
        });
    }
    if let Some(maximum) = required.maximum {
        if actual > maximum {
            return Some(JavaMismatch::TooNew { maximum, actual });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{check, JavaMismatch};
    use crate::game::java::mapping::JavaRequirement;

    #[test]
    fn check_reports_both_directions() {
        let modern = JavaRequirement::table(21, None);
        assert_eq!(check(21, &modern), None);
        assert_eq!(check(25, &modern), None);
        assert_eq!(
            check(17, &modern),
            Some(JavaMismatch::TooOld {
                required: 21,
                actual: 17
            })
        );
        let legacy = JavaRequirement::table(8, Some(8));
        assert_eq!(check(8, &legacy), None);
        assert_eq!(
            check(21, &legacy),
            Some(JavaMismatch::TooNew {
                maximum: 8,
                actual: 21
            })
        );
    }

    #[test]
    fn messages_name_both_sides_and_codes_are_stable() {
        let message = JavaMismatch::TooOld {
            required: 21,
            actual: 8,
        }
        .message();
        assert!(message.contains("Java 21"));
        assert!(message.contains("Java 8"));
        assert_eq!(
            JavaMismatch::TooNew {
                maximum: 8,
                actual: 21
            }
            .code(),
            "java_too_new"
        );
        assert_eq!(
            JavaMismatch::TooNew {
                maximum: 8,
                actual: 21
            }
            .actual(),
            21
        );
    }

    #[test]
    fn advice_names_the_major_and_the_escape_hatch() {
        let advice = JavaMismatch::TooNew {
            maximum: 8,
            actual: 17,
        }
        .advice();
        assert!(advice.contains("Java 8"), "{advice}");
        assert!(advice.contains("java install 8"), "{advice}");
        assert!(advice.contains("--java"), "{advice}");
        assert!(JavaMismatch::TooOld {
            required: 21,
            actual: 17
        }
        .advice()
        .contains("java install 21"));
    }
}
