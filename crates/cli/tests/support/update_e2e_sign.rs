//! 更新 e2e 夹具的**签名**部分（M190）：用 `packaging/sign_manifest.py` 给清单签 `.sig`。
//!
//! 为什么单独一个文件：`support/update_e2e.rs` 已经贴着全仓 400 行红线（M190 r1 复审 P1-3），
//! 而签名这一摊只被 `Fixture::publish` 用到。搬出来是纯搬家，语义未改。
//!
//! 为什么走**真脚本**而不是在测试里再写一份签名：这样这套端到端用例同时是「发布侧签名脚本 ⇒
//! 真二进制验签」的跨实现证据（Python 签、Rust 验）。脚本零第三方依赖，CI runner 与开发机都自带
//! python3/python；找不到 python 时这套用例**直接红**（不静默跳过——跳过等于这条链没有证据）。

use std::path::Path;

/// 测试签名钥的种子（**公开**：只对 `update-test-source` 构建有意义，正式信任表里没有这把钥，
/// 见 `crates/core/src/update/trust.rs::TEST_ONLY`）。
const TEST_KEY_SEED: &str = "ull-m190-test-key-seed-000000001";
/// 测试钥的 `key_id`（与 `trust.rs::TEST_ONLY` 一致）。
const TEST_KEY_ID: &str = "test-only-key";

/// 用 `packaging/sign_manifest.py` 给刚落盘的清单签一份 `.sig`（M190）。
///
/// 为什么走**真脚本**而不是在测试里再写一份签名：这样这套端到端用例同时是「发布侧签名脚本 ⇒
/// 真二进制验签」的跨实现证据（Python 签、Rust 验）。脚本零第三方依赖，CI runner 与开发机都自带
/// python3/python；找不到 python 时这套用例**直接红**（不静默跳过——跳过等于这条链没有证据）。
pub fn write_signature(manifest_path: &Path) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/sign_manifest.py");
    assert!(script.is_file(), "找不到签名脚本：{}", script.display());
    let seed_hex: String = TEST_KEY_SEED
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let secret = format!(r#"{{"key_id":"{TEST_KEY_ID}","seed_hex":"{seed_hex}"}}"#);
    let mut command = None;
    for candidate in ["python3", "python"] {
        if std::process::Command::new(candidate)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            command = Some(candidate);
            break;
        }
    }
    let python = command.unwrap_or_else(|| panic!("这套用例需要 python3/python 跑 {script:?}"));
    let output = std::process::Command::new(python)
        .arg(&script)
        .arg("--sign")
        .arg(manifest_path)
        .env("ULL_MANIFEST_SIGNING_KEY", &secret)
        .output()
        .expect("跑签名脚本");
    assert!(
        output.status.success(),
        "签名脚本失败：stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        manifest_path
            .with_file_name(format!(
                "{}.sig",
                manifest_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ))
            .is_file(),
        "签名脚本没产出 .sig"
    );
}
