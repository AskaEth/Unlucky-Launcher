//! M390：`set_yolo` 是**键-scoped** 的落盘入口——只写 `ai.yolo`，别的键一个都不许动。
//!
//! 从 `store_tests.rs` 拆出来（那边 210 行 + 这条用例会顶到 400 行硬线上；全仓单文件 ≤400
//! 是 `guards.yml` 的门禁）。
//!
//! | 用例 | 守什么 | 变异（改成什么会红） |
//! |---|---|---|
//! | `yolo_round_trips_and_leaves_the_other_keys_alone` | 只写一个键，密钥/端点/模型不受影响 | `set_yolo` 改调 `save()` 写整张表单 |
//! | `yolo_reads_back_from_a_fresh_file` | 新文件里没这一键 ⇒ 读回来是关（fail-closed） | 缺省 `true` |

use super::{load, save, set_yolo};
use crate::pages::settings::ai::state::{ProviderId, Secret, State};

fn temp_config(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ull-ai-yolo-{label}-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(dir.join("config")).expect("临时目录");
    let path = dir.join("config/launcher.json");
    let _ = std::fs::remove_file(&path);
    path
}

fn draft() -> State {
    State {
        enabled: true,
        provider: ProviderId::Anthropic,
        base_url: "https://api.anthropic.com".to_string(),
        model: "claude-sonnet-4".to_string(),
        key: Secret::new("sk-ant-secret"),
        dirty: true,
        storage: true,
        ..State::default()
    }
}

#[test]
fn yolo_round_trips_and_leaves_the_other_keys_alone() {
    let path = temp_config("roundtrip");
    let saved = save(&path, &draft()).expect("先写一份完整配置");
    assert!(!saved.yolo, "`save` 也要把草稿里的 yolo 写进去（缺省关）");

    let on = set_yolo(&path, true).expect("拨开");
    assert!(on.yolo);
    assert_eq!(on.provider, saved.provider, "别的键一个都不许动");
    assert_eq!(on.base_url, saved.base_url);
    assert_eq!(on.model, saved.model);
    assert_eq!(on.key_chars, saved.key_chars, "拨开关不许把已存的密钥碰掉");

    // 断言落在**文件字节**上，不落在任何中间对象上。
    let text = std::fs::read_to_string(&path).expect("读文件");
    assert!(text.contains("\"yolo\": true"), "{text}");
    assert!(text.contains("\"api_key\": \"sk-ant-secret\""), "{text}");
    assert!(
        text.contains("\"base_url\": \"https://api.anthropic.com\""),
        "{text}"
    );

    let off = set_yolo(&path, false).expect("拨回去");
    assert!(!off.yolo);
    let text = std::fs::read_to_string(&path).expect("读文件");
    assert!(text.contains("\"yolo\": false"), "{text}");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

#[test]
fn yolo_reads_back_from_a_fresh_file() {
    let path = temp_config("fresh");
    // 新文件里没有 `ai.yolo` 这一键：读回来必须是**关**（判定链那边也按关处理）。
    assert!(!load(&path).yolo, "缺省必须是关（fail-closed）");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}
