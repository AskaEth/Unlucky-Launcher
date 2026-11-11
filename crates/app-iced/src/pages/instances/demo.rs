//! 实例页的**演示数据 → 视图模型**转换（M84 的出图基线靠这份数据）。
//!
//! 只被 [`crate::pages::demo_model`] 用：产品路径的转换在 `app/view.rs`（presenter 快照 → 模型）。
//! 两份数据源喂同一个渲染函数，这是 [`crate::pages::model`] 存在的理由。

use unlucky_i18n::{Catalog, Key, Lang};
use unlucky_presenter::dto::LaunchArgsView;

use super::super::model::{Detail, Instances, LaunchFacts, Meta, Row};
use super::data::{DEMO, NOTE, SELECTED};
use super::state::LaunchForm;

/// 演示路径的目录：**恒英文**。
///
/// 为什么不是 `model.lang`：本函数只被 `pages::demo_model` 调用，而那条路（`pages_demo` harness /
/// 单测）走 `Model::new`，`lang` 恒为 `Lang::En`（harness 没有换语言的开关）。产品路径不读这里
/// ——它走 `app/view.rs` 的快照转换。等 harness 有 `--lang` 时把这一行换成参数即可。
fn catalog() -> Catalog {
    Lang::En.catalog()
}

/// 元信息卡的标签（与 `app/view/instances.rs` 的 `detail_of` 同一批 key，只是这里恒英文）。
fn label(key: Key) -> &'static str {
    catalog().text(key)
}

/// 把 M84 的演示条目 [`DEMO`] 转成视图模型（真页面走 `app/view.rs` 的 `AppState` 转换）。
pub(crate) fn demo() -> Instances {
    let rows = DEMO
        .iter()
        .map(|entry| Row {
            id: entry.name.to_string(),
            name: entry.name.to_string(),
            supporting: format!("{} · {}", entry.version, entry.loader),
            status: None,
        })
        .collect();

    let entry = &DEMO[SELECTED];
    let detail = Detail {
        id: entry.name.to_string(),
        name: entry.name.to_string(),
        subtitle: format!("{} · {}", entry.version, entry.loader),
        meta: vec![
            meta(label(Key::InstancesMetaVersion), entry.version),
            meta(label(Key::InstancesMetaLoader), entry.loader),
            meta(label(Key::InstancesMetaMemory), entry.memory),
            meta(label(Key::InstancesMetaJava), entry.java),
            meta(label(Key::InstancesMetaMods), entry.mods),
            meta(label(Key::InstancesMetaLastPlayed), entry.last_played),
        ],
        note: Some(NOTE.to_string()),
        status: None,
        can_launch: true,
        can_stop: false,
        launch: launch_facts(),
    };

    Instances {
        rows,
        detail: Some(detail),
    }
}

fn meta(label: &str, value: &str) -> Meta {
    Meta {
        label: label.to_string(),
        value: value.to_string(),
    }
}

/// 启动参数区的演示事实（M157 的 D30 出图夹具）。
///
/// `ULL_LAUNCH_ARGS_FIXTURE=gated` 出「版本不够、三态灰化」那一屏；缺省（`open`）是
/// 「26.2 实例 + Vulkan 可选」的正常态。**这是出图夹具，不是产品逻辑**：产品路径的门槛
/// 来自 presenter 快照（core 的 `graphics_backend_allowed`），与这个环境变量无关。
pub(crate) fn launch_facts() -> LaunchFacts {
    let gated = gated_fixture();
    LaunchFacts {
        allowed: !gated,
        mc_version: if gated { "1.20.4" } else { "26.2-snapshot-2" }.to_string(),
        min_version: "26.2-snapshot-2".to_string(),
        memory_effective_mb: if gated { 2048 } else { 4096 },
        java_effective: "C:/Java/temurin-21/bin/java.exe".to_string(),
        backend_effective: if gated { "default" } else { "vulkan" }.to_string(),
        options: ["default", "opengl", "vulkan"]
            .iter()
            .map(|id| (*id).to_string())
            .collect(),
    }
}

/// 演示用的表单初值（页面状态的所有者是 `App`；harness 没有 `App`，自己持有一份）。
///
/// 返回**拥有**的表单而不是 `&'static`：出图/单测各自持有一份即可（一帧借用一次），
/// 而 `static` 会让所有出图路径共享同一份可编辑状态（两个参数的控件现在都是单行
/// `field::text_field` + `String`，不再是早先的多行编辑器）。
pub(crate) fn demo_form() -> LaunchForm {
    LaunchForm::from_view(Some(DEMO[SELECTED].name), &demo_view())
}

fn demo_view() -> LaunchArgsView {
    let gated = gated_fixture();
    LaunchArgsView {
        instance_id: DEMO[SELECTED].name.to_string(),
        jvm_args: "-XX:+UseG1GC".to_string(),
        game_args: "--fullscreen".to_string(),
        memory_mb: (!gated).then_some(4096),
        memory_effective_mb: if gated { 2048 } else { 4096 },
        java_path: String::new(),
        java_effective: "C:/Java/temurin-21/bin/java.exe".to_string(),
        graphics_backend: if gated { "default" } else { "vulkan" }.to_string(),
        graphics_backend_effective: if gated { "default" } else { "vulkan" }.to_string(),
        graphics_backend_allowed: !gated,
        mc_version: if gated { "1.20.4" } else { "26.2-snapshot-2" }.to_string(),
        graphics_backend_min_version: "26.2-snapshot-2".to_string(),
        graphics_backend_options: ["default", "opengl", "vulkan"]
            .iter()
            .map(|id| (*id).to_string())
            .collect(),
    }
}

fn gated_fixture() -> bool {
    std::env::var("ULL_LAUNCH_ARGS_FIXTURE")
        .map(|value| value.trim().eq_ignore_ascii_case("gated"))
        .unwrap_or(false)
}
