//! 实例页的**演示数据 → 视图模型**转换（M84 的出图基线靠这份数据）。
//!
//! 只被 [`crate::pages::demo_model`] 用：产品路径的转换在 `app/view.rs`（presenter 快照 → 模型）。
//! 两份数据源喂同一个渲染函数，这是 [`crate::pages::model`] 存在的理由。

use unlucky_i18n::{Catalog, Key, Lang};

use super::super::model::{Detail, Instances, Meta, Row};
use super::data::{DEMO, NOTE, SELECTED};

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
