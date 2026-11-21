//! 「现在该推荐哪一档」（M197）：`java.auto_install` 的唯一消费者。
//!
//! 从 `downloads.rs` 拆出来（那个文件加了作业槽与事件映射之后越过 400 行硬线）。
//! 纯函数：只读「当前实例要求 + 已扫到的运行时 + 这个键」，不联网、不落盘。

use unlucky_core::game::java::catalog;

use crate::pages::settings::java::context::Requirement;
use crate::pages::settings::java::row::RuntimeRow;

/// 「现在该推荐哪一档」：`java.auto_install` 的真实消费者。
///
/// 语义（就是界面上那句 hint）：**开着**时，只有当「扫到的 JDK 里没有一个满足当前实例的要求」
/// （没有实例时 = 一个 JDK 都没有）才推荐一档；推荐的是当前实例最低要求对齐到目录的那一档
/// （[`catalog::default_major`]，没有实例时 = 基准 17）。**关掉**就一律不推荐，用户自己挑。
///
/// 为什么不直接自动开始下载：那是 180 MiB 起步、且用户可能只想手动指定 JAVA_HOME。
/// 「替你选好一档 + 一句话说明」把决定权留在用户手上，同时把「该装哪个」这件知识给出去。
#[must_use]
pub fn recommend(
    required: Option<&Requirement>,
    runtimes: &[RuntimeRow],
    auto_install: bool,
) -> Option<u32> {
    if !auto_install {
        return None;
    }
    let satisfied = match required {
        Some(required) => runtimes.iter().any(|runtime| {
            runtime
                .major
                .is_some_and(|major| required.satisfied_by(major))
        }),
        None => !runtimes.is_empty(),
    };
    if satisfied {
        return None;
    }
    Some(catalog::default_major(
        required.map(|required| required.minimum),
    ))
}
