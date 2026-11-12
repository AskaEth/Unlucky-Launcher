//! `client_id` 的判据（从 `client_id.rs` 拆出来守 400 行硬线）。
//!
//! 两块：①「缺 id」文案与日志行的措辞纪律（M117）；②**M178 守卫**——client id 只能构建期注入，
//! 微软轨源码里不许出现 UUID 形状的字面量（复审 r1 P2-3 加严：`concat!` / 跨行 / `format!`
//! 三种拼法都要抓得住，且不许误杀 `session.rs` 里那个 profile UUID）。

use std::sync::Mutex;

use super::{
    client_id_status, resolve_client_id, ClientIdSource, CLIENT_ID_BUILD, CLIENT_ID_ENV,
    MISSING_CLIENT_ID_MESSAGE,
};
use crate::auth::error::AuthError;
use crate::auth::microsoft::MicrosoftAuth;

/// 环境变量是**进程级**的：动它的用例必须串行，否则并行跑会互相摘/设，结论随机。
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// 用例结束后把环境变量还原（panic 也还原：`Drop` 跑得到）。
struct EnvGuard {
    saved: Option<String>,
}

impl EnvGuard {
    fn clear() -> Self {
        let saved = std::env::var(CLIENT_ID_ENV).ok();
        std::env::remove_var(CLIENT_ID_ENV);
        Self { saved }
    }

    fn set(value: &str) -> Self {
        let saved = std::env::var(CLIENT_ID_ENV).ok();
        std::env::set_var(CLIENT_ID_ENV, value);
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.saved.take() {
            Some(value) => std::env::set_var(CLIENT_ID_ENV, value),
            None => std::env::remove_var(CLIENT_ID_ENV),
        }
    }
}

#[test]
fn missing_client_id_message_says_unconfigured_not_unimplemented() {
    // 抓得住：文案退回「本期未支持」这类说法（用户实跑到的那个 bug），
    //         或者只说「需要 id」却不给怎么配（用户下一步就卡在这里）。
    // 抓不住：文案好不好读——那要靠 review。
    assert!(MISSING_CLIENT_ID_MESSAGE.contains("客户端 id"));
    assert!(MISSING_CLIENT_ID_MESSAGE.contains("未配置"));
    assert!(!MISSING_CLIENT_ID_MESSAGE.contains("未支持"));
    assert!(!MISSING_CLIENT_ID_MESSAGE.contains("未实现"));
    assert!(MISSING_CLIENT_ID_MESSAGE.contains(CLIENT_ID_ENV));
    assert!(MISSING_CLIENT_ID_MESSAGE.contains("cargo build"));
    // 硬约束：这句话进日志、进横幅、进错误，**示例一律是 `<id>` 占位**，不出现真 id。
    assert!(MISSING_CLIENT_ID_MESSAGE.contains("<id>"));
}

#[test]
fn from_env_without_client_id_is_a_config_error_not_a_feature_gap() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let _guard = EnvGuard::clear();
    let result = MicrosoftAuth::from_env();
    match result {
        Err(AuthError::Unsupported(message)) => {
            // 机读码保持 `unsupported`（CLI `--json` 与既有调用方靠它分流），
            // 但**人读文案**必须说「未配置」。
            assert_eq!(message, MISSING_CLIENT_ID_MESSAGE);
            assert!(!message.contains("未支持"));
        }
        // 构建期注入过 id 时唯一合法（`ULL_MSA_CLIENT_ID` 也设进 `cargo test` 的环境就会走到这里；
        // 官方发布构建即烘了 id，但门禁里的 `cargo test` 不带该变量，故常态走上面那条。
        // 注意：把该变量导出到跑测试的 shell 会让 presenter 的 M132 入口锚**故意红**
        // （它要求「两处都没有 id」的构建，见 crates/presenter/src/host/accounts/tests.rs）。
        Ok(_) => assert!(
            CLIENT_ID_BUILD.is_some_and(|value| !value.trim().is_empty()),
            "没配 id 却构造成功"
        ),
        Err(other) => panic!("应是 Unsupported，实际 {other:?}"),
    }
}

#[test]
fn env_client_id_wins_over_build_and_the_log_says_so() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let _guard = EnvGuard::set("0000-test-env-id");

    let (id, source) = resolve_client_id().expect("环境变量给了 id");
    assert_eq!(id, "0000-test-env-id");
    // 两者都存在时环境变量优先，且**不告警**，但来源必须是 env。
    assert_eq!(source, ClientIdSource::Env);

    let status = client_id_status();
    assert!(status.is_configured());
    assert_eq!(status.source(), Some(ClientIdSource::Env));
    assert_eq!(
        status.log_line(),
        "[auth] msa client_id=configured source=env"
    );

    // 有 id 时**不再报配置错误**：只验到「进得了登录流程的入口」为止，
    // `from_env` 本身零网络（它只建 reqwest 客户端，不发请求）。
    let auth = MicrosoftAuth::from_env().expect("配了 id 就该构造成功");
    assert!(auth.endpoints().profile.contains("minecraftservices.com"));
}

#[test]
fn missing_client_id_log_line_is_machine_readable() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let _guard = EnvGuard::clear();
    if CLIENT_ID_BUILD.is_some_and(|value| !value.trim().is_empty()) {
        // 构建期注入过 id：这条没有「缺配置」可言，跳过。
        return;
    }
    let status = client_id_status();
    assert!(!status.is_configured());
    assert_eq!(status.source(), None);
    assert_eq!(
        status.log_line(),
        "[auth] msa client_id=missing source=none"
    );
}

#[test]
fn blank_env_value_counts_as_missing() {
    // 抓得住：`ULL_MSA_CLIENT_ID=""`（空串/全空白）被当成配好了，进而拿空 id 去联网。
    let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let _guard = EnvGuard::set("   ");
    if CLIENT_ID_BUILD.is_some_and(|value| !value.trim().is_empty()) {
        assert_eq!(
            resolve_client_id().map(|(_, source)| source),
            Some(ClientIdSource::Build),
            "空白环境变量必须让位给构建期注入"
        );
    } else {
        assert!(resolve_client_id().is_none());
    }
}

/// **M178 守卫**：client id 只能构建期/运行期注入，**微软轨源码里不许出现任何真实 id**。
///
/// 抓得住：为了让登录「开箱可用」而把一个真 client id 写进源码（或写进本模块的默认值）。
/// 抓不住：`.github/workflows` 里 Secret 的接线（那不在本 crate 的范围内，靠发布链自己的
/// `packaging/check_release_semantics.py` 与 Actions 日志看）。
#[test]
fn no_client_id_is_hardcoded_in_the_microsoft_sources() {
    // 判据一：本模块的 id 只能来自 `option_env!`（构建期注入）。
    let manifest = env!("CARGO_MANIFEST_DIR");
    let source = std::fs::read_to_string(format!("{manifest}/src/auth/microsoft/client_id.rs"))
        .expect("读 client_id.rs");
    assert!(
        source.contains("option_env!(\"ULL_MSA_CLIENT_ID\")"),
        "client id 的构建期注入点不见了"
    );
    // 判据二：`auth/microsoft/` 下的任何源码都不许出现 UUID 形状的字面量
    // （Azure 客户端 id 就是 UUID）。扫**全部文件**，所以把 id 挪到别的模块同样会红。
    // 范围只到 `auth/microsoft/`：`auth/offline.rs` 里那个带连字符的 UUID 是离线名字的
    // 测试向量，与 client id 无关。
    let mut offenders = Vec::new();
    let mut stack = vec![format!("{manifest}/src/auth/microsoft")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path.display().to_string());
                continue;
            }
            if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                if contains_uuid_literal(&text) {
                    offenders.push(path.display().to_string());
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "这些源文件里出现了 UUID 形状的字面量（client id 只能构建期注入）：{offenders:?}"
    );
}

/// 源码里有没有「Azure 客户端 id 形状」的字面量（复审 r1 P2-3 加严）。
///
/// 两条判据，覆盖复审点名的三种拼法：
/// 1. **带连字符**（`8-4-4-4-12`）：把全部字面量首尾相接后扫，于是
///    `concat!("83fd9b2f-", "da89-…")`、跨行相邻两段、单段完整 UUID 都能抓到；
/// 2. **无连字符的 32 位十六进制、且由 ≥2 个**连续**的「纯十六进制字面量**拼起来
///    （`format!("{}-{}-{}-{}", "83fd9b2f", "da89", …)` 这类逐段拼法）。
///
/// 为什么判据 2 要求 ≥2 段：`session.rs` 的测试数据里有 profile UUID
/// （`"0f8fad5bd9cb469fa16570867728950e"`）——单个字面量的 32 位 hex 是**档案 id**，
/// 不是 client id；只有「拼出来的」才可疑。也不能把字面量无分隔地全部相接去数 hex 段：
/// 那样下一个字面量的开头若恰好是 hex 字符（如 `"Bearer"` 的 `Bea`）会把上一段续长造成误杀。
fn contains_uuid_literal(text: &str) -> bool {
    let literals = string_literals(text);
    let joined: String = literals.iter().map(String::as_str).collect();
    if has_dashed_uuid(&joined) {
        return true;
    }
    // 判据 2：连续的「纯 hex 字面量」累计长度 ≥32 且至少两段。
    let mut run_len = 0;
    let mut run_parts = 0;
    for literal in &literals {
        let pure_hex = !literal.is_empty() && literal.bytes().all(|b| b.is_ascii_hexdigit());
        if pure_hex {
            run_len += literal.len();
            run_parts += 1;
            if run_len >= 32 && run_parts >= 2 {
                return true;
            }
        } else {
            run_len = 0;
            run_parts = 0;
        }
    }
    false
}

/// 收集源码里的字符串字面量内容（跳过注释；`\` 转义按「跳一个字符」处理）。
///
/// 为什么跳注释：本模块的文档里就写着 `concat!("83fd9b2f-", "da89-…")` 这类**说明性**例子，
/// 不跳的话自检样本会被自己的文档抓成 offender（实测踩过）。注释里出现 id 不构成「代码里
/// 硬编码 id」，判据只管可执行的字面量。
///
/// 近似处（够用即可）：字符字面量 `'a'` 不当字符串；`r#"…"#` 按普通字面量近似（前缀 `r`
/// 不在引号内，不影响内容提取）。
fn string_literals(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            // 行注释：跳到行尾
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            // 块注释：跳到 `*/`
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
            }
            b'"' => {
                index += 1;
                let start = index;
                let mut escaped = false;
                while index < bytes.len() {
                    let byte = bytes[index];
                    if escaped {
                        escaped = false;
                        index += 1;
                        continue;
                    }
                    match byte {
                        b'\\' => {
                            escaped = true;
                            index += 1;
                        }
                        b'"' => break,
                        _ => index += 1,
                    }
                }
                let end = index.min(bytes.len());
                out.push(String::from_utf8_lossy(&bytes[start..end]).into_owned());
                index += 1;
            }
            _ => index += 1,
        }
    }
    out
}

/// 拼接后的串里有没有 `8-4-4-4-12` 的十六进制段（间距 5-5-5-13）。
fn has_dashed_uuid(text: &str) -> bool {
    fn hex(value: &[u8]) -> bool {
        !value.is_empty() && value.iter().all(u8::is_ascii_hexdigit)
    }
    let bytes = text.as_bytes();
    let dashes: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'-')
        .map(|(index, _)| index)
        .collect();
    dashes.windows(4).any(|window| {
        let (first, last) = (window[0], window[3]);
        let gaps = [
            window[1] - first,
            window[2] - window[1],
            window[3] - window[2],
        ];
        if gaps != [5, 5, 5] || last + 13 > bytes.len() || first < 8 {
            return false;
        }
        hex(&bytes[first - 8..first])
            && hex(&bytes[first + 1..window[1]])
            && hex(&bytes[window[1] + 1..window[2]])
            && hex(&bytes[window[2] + 1..last])
            && hex(&bytes[last + 1..last + 13])
    })
}

/// 判据自检：三种拼法都要抓得住，且不能把普通代码当 client id。
///
/// 没有这条，扫描可能因为判据本身坏掉而永远为真（假绿）。
/// 样本**运行时拼、且源码里不出现十六进制字面量**（用 `char` 数组拼）：否则这条自检的
/// 样本本身会被扫描抓成 offender——自指陷阱（实测踩过一次）。
#[test]
fn the_uuid_literal_detector_bites_three_shapes() {
    let part = |chars: &[char]| chars.iter().collect::<String>();
    let head = part(&['8', '3', 'f', 'd', '9', 'b', '2', 'f']);
    let p2 = part(&['d', 'a', '8', '9']);
    let p3 = part(&['4', '1', 'f', '7']);
    let p4 = part(&['b', '4', '1', 'b']);
    let tail = part(&['1', '3', '7', '4', '2', 'a', '7', '3', '8', '4', '1', 'c']);
    let uuid = format!("{head}-{p2}-{p3}-{p4}-{tail}");

    // ① 单个完整字面量（基线）。
    let plain = format!("let id = \"{uuid}\";");
    assert!(contains_uuid_literal(&plain), "{plain}");
    // ② `concat!` 拼出来（复审点名的形态之一）。
    let concatenated = format!("const ID: &str = concat!(\"{head}-\", \"{p2}-{p3}-{p4}-{tail}\");");
    assert!(contains_uuid_literal(&concatenated), "{concatenated}");
    // ③ 跨行两段字面量。
    let split_lines =
        format!("let id = concat!(\n    \"{head}-{p2}-\",\n    \"{p3}-{p4}-{tail}\"\n);");
    assert!(contains_uuid_literal(&split_lines), "{split_lines}");
    // ④ `format!` 逐段拼（无连字符落点，靠「连续纯 hex 字面量累计 ≥32 且 ≥2 段」抓）。
    let formatted = format!(
            "let id = format!(\"{{}}-{{}}-{{}}-{{}}-{{}}\", \"{head}\", \"{p2}\", \"{p3}\", \"{p4}\", \"{tail}\");"
        );
    assert!(contains_uuid_literal(&formatted), "{formatted}");

    // 反例：这些都不许误杀。
    assert!(!contains_uuid_literal("// 0000-test-client"));
    assert!(!contains_uuid_literal("a-b-c-d"));
    assert!(!contains_uuid_literal(
        "let uri = \"http://localhost/auth?client_id=not-a-uuid\";"
    ));
    // 单个字面量的 32 位 hex = 既有测试数据（`session.rs` 的 profile UUID），不算 client id。
    let profile = format!("let profile_id = \"{head}{p2}{p3}{p4}{tail}\";");
    assert!(!contains_uuid_literal(&profile), "{profile}");
    // 段长不对的连字符串不算（`8-4-4-4-12` 之外一律放行）。
    assert!(!contains_uuid_literal(
        "let x = \"abcdefgh-1234-5678-9012-12345678901\";"
    ));
}
