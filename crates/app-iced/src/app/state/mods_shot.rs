//! 出图取证用的 MODS 夹具（`ULL_MODS_FIXTURE=1`）。
//!
//! # 它解决什么
//!
//! D30 的 MODS 区截图需要一屏**有内容的列表**。产品路径的列表来自后台线程（列 `mods/` → 快照），
//! 而截图通道在第 3 帧就出图（`CAPTURE_AT_FRAME`）：那条后台往返来不及完成，截出来会是
//! 「正在读取 mods/…」。M157 的启动参数区、M222 的下载页、M223 的导入都是同一个问题，
//! 处理方式一致：**状态注入**，且注入点放在「快照 → 页面模型」之前，于是转换逻辑照样被执行。
//!
//! # 边界（不要把它读成产品能力）
//!
//! - 只在环境变量存在时生效；产品路径读不到它 ⇒ 一帧都不影响；
//! - 它注入的是**快照**（[`ModListView`]），不是页面模型——`Mods::from_state` 的转换、
//!   计数、`.old` 徽标、在飞置灰走的都是产品代码；
//! - 真实链路由两条端到端用例钉住（`host::mods::tests` 在真盘上列目录 + 启停）。

/// 夹具开关是否打开（`ULL_MODS_FIXTURE` 非空且不为 `0`）。
#[must_use]
pub fn enabled() -> bool {
    std::env::var("ULL_MODS_FIXTURE")
        .map(|value| {
            let value = value.trim();
            !value.is_empty() && value != "0"
        })
        .unwrap_or(false)
}

/// 一份固定快照（实例 id 用调用方给的那个）。
///
/// `dir` 故意留空：夹具不碰路径（这一版页面也不画它），写一个假路径反而会被当成"真的派生结果"。
#[must_use]
pub fn view(instance: &str) -> unlucky_presenter::dto::ModListView {
    use unlucky_presenter::dto::{ModEntryView, ModListView};

    let entry = |file: &str, name: &str, enabled: bool, bytes: u64, old: bool| ModEntryView {
        file: file.to_string(),
        name: name.to_string(),
        enabled,
        bytes,
        old,
    };
    // 夹具不碰路径：`dir` 留空（页面这一版不画它，它只是 DTO 的一部分）。
    let dir = String::new();

    ModListView {
        instance: instance.to_string(),
        dir,
        exists: true,
        entries: vec![
            entry(
                "sodium-fabric-0.5.11.jar",
                "sodium-fabric-0.5.11.jar",
                true,
                1_048_576,
                false,
            ),
            entry(
                "iris-fabric-1.8.5.jar",
                "iris-fabric-1.8.5.jar",
                true,
                2_621_440,
                false,
            ),
            entry(
                "jei-1.20.4-forge-17.3.0.49.jar.disabled",
                "jei-1.20.4-forge-17.3.0.49.jar",
                false,
                1_363_148,
                false,
            ),
            entry(
                "aether-1.20.1-1.0.0.jar.old",
                "aether-1.20.1-1.0.0.jar",
                true,
                7_340_032,
                true,
            ),
            entry(
                "create-1.20.1-0.5.1.f.jar.disabled",
                "create-1.20.1-0.5.1.f.jar",
                false,
                4_194_304,
                false,
            ),
            entry(
                "fabric-api-0.92.2+1.20.4.jar",
                "fabric-api-0.92.2+1.20.4.jar",
                true,
                2_097_152,
                false,
            ),
        ],
        skipped: Vec::new(),
        running: false,
        known: true,
    }
}
