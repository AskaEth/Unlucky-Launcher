//! pack 用例的共用夹具：临时目录、文件写入、host 泵。
//!
//! 拆出去只为一件事：`tests.rs` 已经 429 行（超 400 硬线），而这些 helper 与任何一条断言无关。
//! `tests.rs` 里各条用例的「抓得住什么」写在用例自己的注释里，这里只放机械动作。

use std::path::PathBuf;

/// 每个用例一个独立临时目录（用完删）。
pub(super) fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ull-m113-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

pub(super) fn write(path: &std::path::Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, bytes).expect("write");
}

/// 泵一次 host：把后台任务收成事件并取走。
pub(super) fn pump(host: &mut crate::host::CoreHost) -> Vec<crate::dto::AppEvent> {
    use crate::dto::Backend;
    host.tick(std::time::Duration::from_millis(1));
    host.drain_events()
}

/// 泵到 `done` 为真（最多 ~3 s），返回期间收到的全部事件。
pub(super) fn pump_until(
    host: &mut crate::host::CoreHost,
    done: impl Fn(&[crate::dto::AppEvent]) -> bool,
) -> Vec<crate::dto::AppEvent> {
    let mut all = Vec::new();
    for _ in 0..600 {
        all.extend(pump(host));
        if done(&all) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    all
}

pub(super) fn pack_events(events: &[crate::dto::AppEvent]) -> Vec<crate::dto::PackEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            crate::dto::AppEvent::Core(crate::dto::CoreEvent::Pack(inner)) => {
                Some((**inner).clone())
            }
            _ => None,
        })
        .collect()
}
