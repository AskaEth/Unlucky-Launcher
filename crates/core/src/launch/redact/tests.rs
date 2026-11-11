//! `launch::redact` 的判据测试：抹得掉真凭据，抹不着正常文本。
//!
//! 三组：**正例**（必须抹，占位符形状固定）、**误杀**（M239 实测的形状，必须逐字原样）、
//! **边界**（新判据最容易被拼出来的形状，以及 `has_*` / `mask_*` 两个入口的一致性）。
//! 语料组拿 M239 的回归语料（68 文件 / 243 行）当钉死的输入，要求一行都不许改。
//!
//! BEARERDOT（token 后面跟句读，M243）单独一页：`tests/bearerdot.rs`。

use std::path::{Path, PathBuf};

use super::*;

// `tests.rs` 是以 `#[path]` 挂进来的文件模块，子模块按**它所在目录**（`redact/`）解析，
// 所以这里写全相对路径，文件仍归在 `tests/` 目录下。
#[path = "tests/bearerdot.rs"]
mod bearerdot;

/// 真实形状的 JWT（`header.payload.signature`，三段各 ≥8）。
const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlLXZhbHVlLXRoYXQtaXMtbG9uZy1lbm91Z2g";
/// 微软 MSA/XSTS 形状的 header（`{"typ":"JWT","alg":"none"}`）。
const MSA: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJub25lIn0.eyJ4dWlkIjoiMjUzNTQyOTk4OTg4OTg5OCJ9.c2lnbmF0dXJlLXZhbHVlLXRoYXQtaXMtbG9uZw";
/// 64 位 sha256（hex）。
const SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn scrub(text: &str) -> String {
    Redactor::new().scrub(text).into_owned()
}

// ───────────────────────── 正例：真凭据仍被抹 ─────────────────────────

#[test]
fn jwts_are_masked_in_every_log_shape() {
    let cases = [
        (JWT.to_string(), TOKEN_PLACEHOLDER.to_string()),
        (MSA.to_string(), TOKEN_PLACEHOLDER.to_string()),
        (
            format!("authorization: Bearer {JWT}, retry=1"),
            format!("authorization: Bearer {TOKEN_PLACEHOLDER}, retry=1"),
        ),
        // `键=` 粘在 token 前面：键留下、token 照抹（`=` 是赋值边界，不是 token 本体）
        (
            format!("-DaccessToken={JWT}"),
            format!("-DaccessToken={TOKEN_PLACEHOLDER}"),
        ),
        (
            format!("--accessToken {MSA} --uuid 0123"),
            format!("--accessToken {TOKEN_PLACEHOLDER} --uuid 0123"),
        ),
        // 藏在 URL 路径里也照抹（只有主机名被放行）
        (
            format!("https://resources.download.minecraft.net/{JWT}"),
            format!("https://resources.download.minecraft.net/{TOKEN_PLACEHOLDER}"),
        ),
        // 标准 base64 的三段式（带大写）：不是 URL 形状，照抹
        (
            "QWxsLmJhc2U2NA.QWJjZGVmZ2hpamtsbW5vcA.ZGVmaWdoaWprbG1ub3BxcnN0dXZ3eHl6".to_string(),
            TOKEN_PLACEHOLDER.to_string(),
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(scrub(&input), expected, "输入：{input}");
    }
}

#[test]
fn opaque_secrets_are_masked_in_every_log_shape() {
    let cases = [
        (format!("sha256={SHA256}"), OPAQUE_PLACEHOLDER.to_string()),
        (SHA256.to_string(), OPAQUE_PLACEHOLDER.to_string()),
        ("A".repeat(64), OPAQUE_PLACEHOLDER.to_string()),
        (
            "dGhpcyBpcyBhIHZlcnkgbG9uZyBzZWNyZXQga2V5IHRoYXQgaXMgYmFzZTY0IGVuY29kZWQ=".to_string(),
            OPAQUE_PLACEHOLDER.to_string(),
        ),
        // 带 `/` 与 `+` 的普通 base64：随机大小写，不是路径形状
        (
            "aB3xK9mQ2pL5vN8r/W1tY4uI7oP0sD6fGhJ2kL5zX8cV1bN4mQ7wE3rT6yU9iO2pA5sD8fG1hJ"
                .to_string(),
            OPAQUE_PLACEHOLDER.to_string(),
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(scrub(&input), expected, "输入：{input}");
    }
}

#[test]
fn registered_secrets_and_home_still_apply() {
    let redactor = Redactor::new()
        .with_secret("SECRET-TOKEN-123456")
        .with_home_str("/home/alice");
    let scrubbed = redactor
        .scrub("/home/alice/game.log: token=SECRET-TOKEN-123456")
        .into_owned();
    assert_eq!(
        scrubbed, "<home>/game.log: token=<redacted:token>",
        "登记过的明文值走替换路径，与形状启发式无关"
    );
    assert!(Redactor::new().is_empty(), "空脱敏器不该登记任何规则");
}

// ───────────────────────── 误杀：必须逐字原样 ─────────────────────────

/// M239 的 ①②③：品牌串 / 属性串（`=` 是赋值边界）。
#[test]
fn brand_and_property_strings_survive() {
    for text in [
        "-Dminecraft.launcher.brand=UnluckyLauncher",
        "-Dminecraft.launcher.brand=minecraft",
        "-Dminecraft.launcher.version=0.2.3",
        "-Dsome.prop=com.example.UnluckyLauncher.ClientMain",
        "com.mojang.authlib.GameProfile",
        "net.minecraft.launchwrapper.Launch.launch(Launch.java:135)",
        "1.20.1-forge-47.2.0-beta.20250115.123456",
        "Service.Unavailable.today",
        "xbl.xsts.entitlements.profile",
    ] {
        assert_eq!(scrub(text), text, "误杀：{text}");
    }
}

/// 老规则的负例（M240 重构时被丢掉过，复审 P2-3 要求找回）：分段太短 / 总长不够 / 段数不够。
#[test]
fn dotted_token_rule_does_not_eat_normal_text() {
    for text in [
        "Service.Unavailable.today",
        "1.2.0",
        "xbl.xsts.entitlements.profile",
        "eyJhbGciOiJIUzI1NiJ9.short.abc",
    ] {
        assert!(!has_dotted_token(text), "{text}");
        assert_eq!(scrub(text), text, "{text}");
    }
}

/// 整条 HMCL 形状的 argv（`--dry-run` 的面）：一处都不许动。
#[test]
fn hmcl_shaped_argv_survives_verbatim() {
    let argv = "javaw.exe -Dminecraft.launcher.brand=UnluckyLauncher -Dminecraft.launcher.version=0.2.3 \
                -cp /home/alice/.minecraft/libraries/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-universal.jar \
                net.minecraft.launchwrapper.Launch";
    assert_eq!(scrub(argv), argv);
}

/// M239 的 U1–U6：CDN / 镜像主机名（判据 = 首段是 DNS 主机名 → 只判路径那半）。
#[test]
fn cdn_and_mirror_urls_survive() {
    for text in [
        "https://resources.download.minecraft.net/ab/cdef",
        "https://bmclapi2.bangbang93.com/assets/ab/abcdef0123456789abcdef",
        "http://textures.minecraft.net/texture/8f4a1b2c3d4e5f60718293a4b5c6d7e8",
        "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar",
        "https://api.minecraftservices.com/entitlements/mcstore",
        "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge.jar",
        // 40 字符的纯主机名形状：DNS 形 → 靠首段主机名豁免（不是靠大写判据，
        // 大写判据只管非 DNS 形状，归属见 `lowercase_dotted_names_are_spared` 的 doc）
        "resources.download.minecraft.example-cdn",
        // 短域名 + 长路径（内容全是词形名字）
        "https://short.host/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "mirror.example.com/downloads/0123456789abcdef0123456789abcdef0123456789abcdef",
    ] {
        assert_eq!(scrub(text), text, "URL/主机名被抹：{text}");
    }
}

/// M239 的 O1–O3 / ⑥'：长路径、Java 包名路径（段名是词形，不是随机串）。
#[test]
fn long_paths_and_class_paths_survive() {
    for text in [
        "Java Path: /home/alice/.minecraft/runtime/java-runtime-delta/linux/java-runtime-delta/bin/java",
        "java.lang.NoClassDefFoundError: org/spongepowered/asm/mixin/transformer/FabricMixinTransformerProxy",
        "/srv/games/minecraft/instances/AllTheMods10/mods/sophisticatedbackpacks-universal.jar",
        "/var/folders/xy/0123456789abcdef0123456789abcdef/T/unlucky/launch",
        "net/minecraftforge/fml/loading/FMLEnvironment",
    ] {
        assert_eq!(scrub(text), text, "路径被抹：{text}");
    }
}

/// 语料组：M239 的回归语料，形状规则**一行都不许改**。
///
/// **只守一个方向**：它证明"该保留的保留了"，**不能**拿来背书"正例仍被抹"——正例那一方向
/// 由 `jwts_are_masked_in_every_log_shape` / `opaque_secrets_are_masked_in_every_log_shape` 显式断言。
#[test]
fn sample_corpus_is_untouched() {
    let files = corpus_files();
    let mut lines = 0usize;
    let mut changed = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        for (idx, line) in text.lines().enumerate() {
            lines += 1;
            let out = scrub(line);
            if out != line {
                changed.push(format!("{}:{} → {out}", file.display(), idx + 1));
            }
        }
    }
    assert_eq!(files.len(), 68, "语料文件数变了（M239 口径 68）");
    assert_eq!(lines, 243, "语料行数变了（M239 口径 243）");
    assert!(changed.is_empty(), "形状规则改了语料行：{changed:#?}");
}

// ───────────────────────── 边界 ─────────────────────────

/// 判据的缝合线：路径里**段本身**够长时照抹，段不长就整条留下。
#[test]
fn a_long_path_component_is_still_masked() {
    // 63 字符的段：与无点号密钥的判定长度差一个字符，留下
    let short = format!("https://short.host/{}", "a".repeat(63));
    assert_eq!(scrub(&short), short);
    // 64 字符的段：形状上与 `sha256=<hex>` 不可区分，抹掉
    let long = format!("https://short.host/{}", "a".repeat(64));
    assert_eq!(
        scrub(&long),
        format!("https://short.host/{OPAQUE_PLACEHOLDER}")
    );
    // 路径里夹一段 64 位 hex：只有那一段消失
    assert_eq!(
        scrub(&format!("/srv/mc/assets/objects/{SHA256}")),
        format!("/srv/mc/assets/objects/{OPAQUE_PLACEHOLDER}")
    );
}

/// 边界：**没有** `/` 的长文件名与无点号密钥同形，只能照抹（已知残留，见报告）。
#[test]
fn a_bare_long_filename_is_still_masked() {
    assert_eq!(
        scrub("sophisticatedbackpacks-universal-fabric-loader-compatible-build-final.jar"),
        format!("{OPAQUE_PLACEHOLDER}.jar")
    );
}

/// 边界：路径里有一段**不是**词形的随机串 → 整条按凭据处理（安全方向）。
#[test]
fn a_path_with_a_random_looking_component_is_masked() {
    let secret = "srv/games/aB3xK9mQ2pL5vN8rW1tY4uI7oP0sD6fGhJ2kL5zX8cV1bN4mQ7wE3rT6yU9";
    assert_eq!(scrub(secret), OPAQUE_PLACEHOLDER);
}

/// 边界：URL 主机名放行**不**等于整个 run 放行——JWT 摆在路径里照样抹；
/// 而主机名后面的长路径如果本身是词形，整条留下（`--dry-run` 要能复制）。
#[test]
fn url_host_is_exempt_but_the_path_half_is_still_judged() {
    assert_eq!(
        scrub(&format!("https://cdn.example.com/{JWT}")),
        format!("https://cdn.example.com/{TOKEN_PLACEHOLDER}")
    );
    let url = "https://resources.download.minecraft.net/assets/objects/4b9d8b9d4e6f4c9a8b7d6e5f";
    assert_eq!(scrub(url), url);
}

/// 「旧行为抹、新行为留」的三类形状（复审 P2-1：口径写准，别留"抹除能力没有下降"的绝对承诺）。
///
/// 三条都用 **M240 之前的真实代码**（`git show 74be510:crates/core/src/launch/redact.rs` 直接
/// 用 rustc 编成探针）实测过：旧行为输出 `<redacted:token>`，现在逐字原样。前两条是本次要修的
/// 误杀本身，第三条是全小写点分串的代价（理由见 `shape` 模块文档的「旧行为抹、新行为留」）。
#[test]
fn known_narrowings_old_masked_new_keeps() {
    // ① `=` 边界：键把值带过线（旧行为把 `键=值` 粘成一段凑够段数）
    let brand = "-Dminecraft.launcher.brand=UnluckyLauncher";
    assert_eq!(scrub(brand), brand, "旧行为：<redacted:token>");
    // ② 填充不算段长：末段实际 6 字符（旧行为把 `c2lnbm==` 算成 8 ≥ 8 → 抹）
    let padded = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbm==";
    assert_eq!(scrub(padded), padded, "旧行为：<redacted:token>");
    // ② 的另一侧：末段 8 字符仍照抹（占位符在前、填充留在外面）
    assert_eq!(
        scrub("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmFy=="),
        format!("{TOKEN_PLACEHOLDER}==")
    );
    // ③ 全小写点分串：DNS 形的靠主机名豁免、非 DNS 形的靠"必须含大写"（旧行为都会抹）
    let lowercase = "sophisticatedbackpacks.upgrades.reference";
    assert_eq!(scrub(lowercase), lowercase, "旧行为：<redacted:token>");
}

/// "必须含大写"这条判据**自己**在扛的用例（复审 P2-2 的 (b)）。
///
/// 先划清它的实际覆盖面（复审的怀疑是对的）：**DNS 形状**的全小写点分串（`a.b.c` 全小写）
/// 已经被 [`is_dns_host`] 那条主机名豁免救下了，跟大写判据无关。大写判据唯一扛住的是
/// **不是 DNS 形状**的全小写点分串——例如标签里带 `_`（Java 标识符常见写法）、标签超 63 字符、
/// 或 `/` 之前只有一段主机名。下面就是这一类。
///
/// 删掉 `jwt_shaped` 里那条大写判据，**这条测试与 `lowercase_dotted_names_are_spared` 会红，
/// 其余 15 条仍绿**（M240 复审实测：删判据 → 15 passed / 2 failed）。
#[test]
fn the_uppercase_requirement_is_load_bearing() {
    for text in [
        "sophisticated_backpacks.upgrades.reference", // 41 字符，段长 23/8/9，标签带 `_`
        "spongepowered_mixins.injections.callback",   // 40 字符，段长 20/10/8，标签带 `_`
    ] {
        assert_eq!(
            scrub(text),
            text,
            "全小写、非 DNS 形状的点分串被抹（大写判据失效？）：{text}"
        );
    }
}

/// 全小写点分串整体都不该被抹（两类出口各管一半，测试分开写以免把功劳记错）：
/// DNS 形的靠主机名豁免，非 DNS 形的靠"必须含大写"。
#[test]
fn lowercase_dotted_names_are_spared() {
    for text in [
        "sophisticatedbackpacks.upgrades.reference", // DNS 形状 → 主机名豁免
        "libraries.minecraftservices.entitlements",  // DNS 形状 → 主机名豁免
        "sophisticated_backpacks.upgrades.reference", // 非 DNS → 大写判据
    ] {
        assert_eq!(scrub(text), text, "全小写点分串被抹：{text}");
    }
}

/// 两个入口必须一致：`scrub` 用 `has_*` 当闸门，闸门关着就永远不会调 `mask_*`。
#[test]
fn the_has_gate_agrees_with_the_maskers() {
    let cases = [
        JWT,
        MSA,
        "-Dminecraft.launcher.brand=UnluckyLauncher",
        "https://resources.download.minecraft.net/ab/cdef",
        "https://short.host/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "Java Path: /home/alice/.minecraft/runtime/java-runtime-delta/bin/java",
        "java.lang.NoClassDefFoundError: org/spongepowered/asm/mixin/transformer/FabricMixinTransformerProxy",
        SHA256,
        "1.2.0",
        "",
    ];
    for case in cases {
        assert_eq!(
            has_dotted_token(case),
            mask_dotted_tokens(case) != case,
            "has_dotted_token 与 mask_dotted_tokens 不一致：{case}"
        );
        assert_eq!(
            has_opaque_run(case),
            mask_opaque(case) != case,
            "has_opaque_run 与 mask_opaque 不一致：{case}"
        );
    }
}

/// 语料 = `rules.d/samples/**/*.log` + app 壳的两份示例日志（M239 的 68 文件口径）。
fn corpus_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    let mut stack = vec![root.join("src/diagnose/rules.d/samples")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "log") {
                files.push(path);
            }
        }
    }
    for name in ["sample_crash.log", "sample_live.log"] {
        files.push(root.join("../app-iced/src/pages/logs").join(name));
    }
    files.sort();
    files
}
