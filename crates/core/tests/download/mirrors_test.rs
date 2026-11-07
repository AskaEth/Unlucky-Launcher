//! 镜像清单与前缀重写的单测（纯字符串，无网络）。

use unlucky_core::download::adoptium::{parse_adoptium_artifact, JdkFlavor};
use unlucky_core::download::mirrors::{host_of, source_id_for, MirrorTable, PrefixRule};
use unlucky_core::download::region::RegionProfile;
use unlucky_core::download::types::ResourceKind;

const LIBRARIES_URL: &str =
    "https://libraries.minecraft.net/com/google/guava/guava/32.0.0/guava-32.0.0.jar";
const ASSET_URL: &str = "https://resources.download.minecraft.net/ab/abcdef0123456789";
const GITHUB_URL: &str =
    "https://github.com/AskaEth/Unlucky-Launcher/releases/download/v1.0.0/ull-windows-x64.exe";
const ADOPTIUM_URL: &str = "https://github.com/adoptium/temurin17-binaries/releases/download/JDK17.0.13%2B11/OpenJDK17U-jdk_x64_windows_hotspot_17.0.13_11.zip";

#[test]
fn builtin_table_is_embedded_and_complete() {
    let table = MirrorTable::builtin();
    assert!(table.version >= 1);
    assert!(table.rules.len() >= 10, "内置规则太少");
    assert!(table.adoptium.len() >= 3, "院校镜像至少要 TUNA/USTC/SJTU");
    assert!(!table.gh_proxies.is_empty());
    assert!(table.gh_proxies[0].prefix.starts_with("https://gh-proxy"));
}

#[test]
fn prefix_rule_preserves_sub_path() {
    let table = MirrorTable::builtin();
    let rule = table
        .prefix_rule_for(LIBRARIES_URL, ResourceKind::Library)
        .expect("libraries 规则缺失");
    let candidate = rule.apply(LIBRARIES_URL).expect("重写失败");
    assert_eq!(candidate.source, "bmclapi");
    assert_eq!(
        candidate.url,
        LIBRARIES_URL.replace(
            "https://libraries.minecraft.net",
            "https://bmclapi2.bangbang93.com"
        )
    );
}

#[test]
fn asset_rule_injects_path_prefix() {
    let table = MirrorTable::builtin();
    let candidate = table
        .prefix_rule_for(ASSET_URL, ResourceKind::Asset)
        .and_then(|rule| rule.apply(ASSET_URL))
        .expect("资源对象规则缺失");
    assert_eq!(
        candidate.url,
        "https://bmclapi2.bangbang93.com/assets/ab/abcdef0123456789"
    );
}

#[test]
fn direct_mode_puts_official_first() {
    let table = MirrorTable::builtin();
    let region = RegionProfile::direct(0);
    let candidates = table.expand(LIBRARIES_URL, ResourceKind::Library, &region);
    assert_eq!(candidates[0].source, "official");
    assert_eq!(candidates[0].url, LIBRARIES_URL);
    // 镜像仍然保留为兜底（官方失败时能救回来）。
    assert!(candidates
        .iter()
        .any(|candidate| candidate.source == "bmclapi"));
}

#[test]
fn mirror_first_mode_puts_mirror_first() {
    let table = MirrorTable::builtin();
    let region = RegionProfile::mirror_first(0);
    let candidates = table.expand(LIBRARIES_URL, ResourceKind::Library, &region);
    assert_eq!(candidates[0].source, "bmclapi");
    assert_eq!(
        candidates.last().map(|c| c.source.as_str()),
        Some("official")
    );
}

#[test]
fn gh_proxy_only_for_github_hosts_in_domestic_mode() {
    let table = MirrorTable::builtin();
    let domestic = RegionProfile::mirror_first(0);
    let direct = RegionProfile::direct(0);

    let github = table.expand(GITHUB_URL, ResourceKind::GitHubRelease, &domestic);
    assert!(
        github.iter().any(|candidate| candidate.source == "gh_proxy"
            && candidate.url.starts_with("https://gh-proxy.com/")),
        "国内模式下 GitHub 资源必须套 gh-proxy：{github:?}"
    );

    let non_github = table.expand(LIBRARIES_URL, ResourceKind::Library, &domestic);
    assert!(
        non_github
            .iter()
            .all(|candidate| candidate.source != "gh_proxy"),
        "非 GitHub 资源不受 gh-proxy 影响"
    );

    let github_direct = table.expand(GITHUB_URL, ResourceKind::GitHubRelease, &direct);
    assert!(github_direct
        .iter()
        .all(|candidate| candidate.source != "gh_proxy"));
}

#[test]
fn pinned_or_disabled_gh_proxy_is_respected() {
    let table = MirrorTable::builtin();

    // 用户关掉：地区档案显式不带前缀 → 一条 gh_proxy 候选都不出。
    let off = RegionProfile::mirror_first(0).with_gh_proxy(None);
    let candidates = table.expand(GITHUB_URL, ResourceKind::GitHubRelease, &off);
    assert!(candidates.iter().all(|c| c.source != "gh_proxy"));

    // 自定义前缀（设置项写进地区档案）。
    let custom = RegionProfile::mirror_first(0).with_gh_proxy(Some("https://proxy.example/"));
    let candidates = table.expand(GITHUB_URL, ResourceKind::GitHubRelease, &custom);
    let proxy = candidates
        .iter()
        .find(|candidate| candidate.source == "gh_proxy")
        .expect("自定义前缀未生效");
    assert_eq!(proxy.url, format!("https://proxy.example/{GITHUB_URL}"));

    // 清单给设置做默认值。
    assert_eq!(table.default_gh_proxy(), Some("https://gh-proxy.com/"));
}

#[test]
fn adoptium_artifact_parsing_and_mirror_ordering() {
    let parsed = parse_adoptium_artifact(ADOPTIUM_URL).expect("解析 Temurin 产物名失败");
    assert_eq!(parsed.major, "17");
    assert_eq!(parsed.flavor, JdkFlavor::Jdk);
    assert_eq!(parsed.arch, "x64");
    assert_eq!(parsed.os, "windows");
    assert!(parsed.filename.starts_with("OpenJDK17U-jdk_x64_windows"));

    let table = MirrorTable::builtin();
    let region = RegionProfile::mirror_first(0);
    let candidates = table.expand(ADOPTIUM_URL, ResourceKind::JavaRuntime, &region);
    let first = &candidates[0];
    assert_eq!(first.source, "tuna");
    assert!(
        first
            .url
            .starts_with("https://mirrors.tuna.tsinghua.edu.cn/Adoptium/17/jdk/x64/windows/"),
        "院校镜像路径不符合模板：{}",
        first.url
    );
    // 官方仍在候选里（全部镜像不可达时的兜底）。
    assert!(candidates
        .iter()
        .any(|candidate| candidate.source == "official"));
}

/// JRE 产物必须落到镜像的 `/jre/` 子树（否则 4 个镜像全 404，加速白费）。
#[test]
fn adoptium_jre_uses_jre_path_segment() {
    let jre_url = ADOPTIUM_URL.replace("OpenJDK17U-jdk_", "OpenJDK17U-jre_");
    let parsed = parse_adoptium_artifact(&jre_url).expect("解析 JRE 产物名失败");
    assert_eq!(parsed.flavor, JdkFlavor::Jre);
    assert_eq!(
        parsed.mirror_url("https://mirrors.tuna.tsinghua.edu.cn/Adoptium"),
        format!(
            "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/17/jre/x64/windows/{}",
            parsed.filename
        )
    );

    let table = MirrorTable::builtin();
    let candidates = table.expand(
        &jre_url,
        ResourceKind::JavaRuntime,
        &RegionProfile::mirror_first(0),
    );
    let first = &candidates[0];
    assert_eq!(first.source, "tuna");
    assert!(
        first.url.contains("/17/jre/x64/windows/"),
        "JRE 必须走 /jre/ 子树：{}",
        first.url
    );

    // `-jre-17…` 这种兜底形态同样识别。
    let fallback = "https://github.com/adoptium/temurin21-binaries/releases/download/jre-21.0.5%2B11/OpenJDK21U-jre_x64_linux_hotspot_21.0.5_11.tar.gz";
    assert_eq!(
        parse_adoptium_artifact(fallback).map(|artifact| artifact.flavor),
        Some(JdkFlavor::Jre)
    );
}

#[test]
fn custom_rules_take_precedence() {
    let mut table = MirrorTable::builtin();
    table.add_custom_rule(PrefixRule::new(
        "my-mirror",
        "https://libraries.minecraft.net",
        "https://mirror.example/libraries",
    ));
    let candidate = table
        .prefix_rule_for(LIBRARIES_URL, ResourceKind::Library)
        .and_then(|rule| rule.apply(LIBRARIES_URL))
        .expect("自定义规则未生效");
    assert_eq!(candidate.source, "mirror.example");
    assert_eq!(
        candidate.url,
        "https://mirror.example/libraries/com/google/guava/guava/32.0.0/guava-32.0.0.jar"
    );
}

#[test]
fn fallback_rule_keeps_official_first_even_in_domestic_mode() {
    let mut table = MirrorTable {
        version: 1,
        notes: Vec::new(),
        rules: vec![PrefixRule::new(
            "modrinth-accelerator",
            "https://api.modrinth.com",
            "https://cdn.example",
        )
        .fallback()],
        // v3 新增的三张 JDK 源表在这张夹具里必须为空（否则会混进别的候选）。
        adoptium: Vec::new(),
        oracle_openjdk: Vec::new(),
        microsoft_openjdk: None,
        zulu: None,
        gh_proxies: Vec::new(),
    };
    let region = RegionProfile::mirror_first(0);
    let candidates = table.expand(
        "https://api.modrinth.com/v2/project/sodium",
        ResourceKind::ModFile,
        &region,
    );
    assert_eq!(
        candidates[0].source, "official",
        "「仅加速」规则不能顶掉官方"
    );
    assert_eq!(candidates[1].source, "cdn.example");

    // 关掉规则后只剩官方一条。
    table.rules.clear();
    let candidates = table.expand(
        "https://api.modrinth.com/v2/project/sodium",
        ResourceKind::ModFile,
        &region,
    );
    assert_eq!(candidates.len(), 1);
}

#[test]
fn host_and_source_id_helpers() {
    assert_eq!(
        host_of("https://libraries.minecraft.net/a/b"),
        Some("libraries.minecraft.net")
    );
    assert_eq!(
        host_of("https://user:pass@mirror.example:8443/x/y"),
        Some("mirror.example")
    );
    assert_eq!(host_of("http://127.0.0.1:8080/file"), Some("127.0.0.1"));
    assert_eq!(host_of("not-a-url"), Some("not-a-url"));

    assert_eq!(
        source_id_for("https://bmclapi2.bangbang93.com/assets/ab/cd"),
        "bmclapi"
    );
    assert_eq!(
        source_id_for("https://mirrors.tuna.tsinghua.edu.cn/Adoptium/17/x.zip"),
        "tuna"
    );
    assert_eq!(
        source_id_for("https://mirrors.ustc.edu.cn/adoptium/17/x.zip"),
        "ustc"
    );
    assert_eq!(
        source_id_for("https://gh-proxy.com/https://github.com/a/b"),
        "gh_proxy"
    );
    assert_eq!(source_id_for("https://example.org/file"), "example.org");
}
