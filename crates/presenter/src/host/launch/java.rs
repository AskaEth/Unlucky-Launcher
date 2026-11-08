//! 「用哪个 Java」：显式路径（实例 → 全局）> 托管 > JAVA_HOME > PATH（M161 从 `host/launch.rs` 拆出）。
//!
//! 拆出的理由与其它 400 行拆分一致：`host/launch.rs` 到线了，而「挑 Java」本来就是独立一段。
//! 口径没变（见原文件与 `core::game::java` 的文档）：显式路径**先探测**，探测失败按版本要求的
//! 基准主版本兜底——启动路径不该因为探测不到就拒绝启动。

use std::path::PathBuf;

use unlucky_core::launch::request::JavaSelection;
use unlucky_core::platform::paths::ProcessEnv;

use super::super::env::HostEnv;

/// 选中并用起来的 Java（`summary` 给 UI 一行提示）。
pub struct JavaPick {
    /// 交给 adapter 的选择
    pub selection: JavaSelection,
    /// 人读一行
    pub summary: String,
}

/// 选一个能用的 Java：显式路径（实例 → 全局 `java.selected_path`）> 托管 > JAVA_HOME > PATH。
///
/// 显式路径**先探测**（`java -version`）：探测失败不报错，按版本要求的基准主版本兜底——
/// 与自动挑的退路同一口径（启动路径不该因为探测不到就拒绝启动）。
pub fn pick_java(env: &HostEnv, version_id: &str, explicit: Option<&std::path::Path>) -> JavaPick {
    let store = env.store();
    let required = unlucky_core::game::java::requirement_for_version(&env.game, version_id);
    if let Some(path) = explicit {
        let runtime = unlucky_core::game::java::probe::probe(
            path,
            unlucky_core::game::java::probe::JavaOrigin::Instance,
        );
        let major = runtime.major.unwrap_or(required.minimum);
        return JavaPick {
            selection: JavaSelection::new(runtime.binary.clone(), major, runtime.bits),
            summary: format!(
                "实例指定 Java {}（{}）",
                runtime.version.clone().unwrap_or_else(|| major.to_string()),
                path.display()
            ),
        };
    }
    let install_root = store
        .doc()
        .effective_str("java.installRoot")
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(PathBuf::from);
    let roots = unlucky_core::game::java::location::scan_roots(
        env.mode(),
        &env.layout,
        install_root.as_deref(),
        env.platform,
        &ProcessEnv,
    );
    let candidates = unlucky_core::game::java::candidates_in_roots(&roots);
    let picked = unlucky_core::game::java::pick(&candidates, &required).cloned();
    match picked {
        Some(runtime) => {
            let major = runtime
                .major
                .unwrap_or(unlucky_core::game::java::DEFAULT_MAJOR);
            let summary = format!(
                "Java {}（{}{}）",
                runtime.version.clone().unwrap_or_else(|| major.to_string()),
                runtime.origin.category(),
                runtime
                    .vendor
                    .as_ref()
                    .map(|vendor| format!(" · {vendor}"))
                    .unwrap_or_default()
            );
            JavaPick {
                selection: JavaSelection::new(runtime.binary, major, runtime.bits),
                summary,
            }
        }
        None => {
            let binary = PathBuf::from(unlucky_core::game::java::java_binary_name());
            let major = required.minimum;
            JavaPick {
                selection: JavaSelection::new(binary, major, 64),
                summary: format!("PATH 上的 Java（未探测到版本，按 Java {major} 启动）"),
            }
        }
    }
}
