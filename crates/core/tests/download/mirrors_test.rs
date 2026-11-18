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
    // 子路径逐字保留（`/maven` 是镜像侧的固定前缀，不属于子路径）。M210/D1：这条断言此前写成
    // 「换成 `https://bmclapi2.bangbang93.com` 就完事」，把**丢 inject 的 bug 钉成了期望**——实测
    // 无 `/maven` 是 70/70 个库 404 + 9 B，带 `/maven` 回 200 且 sha1 与官方逐字节一致。
    let expected = LIBRARIES_URL.replace(
        "https://libraries.minecraft.net",
        "https://bmclapi2.bangbang93.com/maven",
    );
    assert_eq!(candidate.url, expected);
}

/// **守卫（M210/D1）**：每一条「给 libraries 用」的镜像规则，镜像前缀都必须带 `/maven`。官方库域名与
/// 各 loader 的 maven 在 BMCLAPI 上都挂在 `/maven` 下，而 `apply()` 是纯前缀替换——`to` 里少了这一段
/// 就会把 `com/google/guava/...` 直接拼到镜像根上。抓得住：删掉/改错任一条库镜像规则的 `inject`；
/// 抓不住：`/maven` **写歪**（如 `/maven2`）只能靠行为用例 [`library_urls_expand_to_the_maven_subpath_on_bmclapi`]；
/// `/assets`、`/fabric-meta` 这类结构本就不同的镜像表，以及 `from` 不像 maven 仓库的新库镜像，都不在列。
#[test]
fn every_library_mirror_rule_carries_the_maven_segment() {
    let table = MirrorTable::builtin();
    // 判据：「给 libraries 用」= `from` 指向 maven 仓库（host/路径里带 maven，或就是 Mojang 库域名）。
    // 新增一条库镜像规则时这条自动纳入，不必改测试。
    fn serves_maven(from: &str) -> bool {
        host_of(from).unwrap_or_default().contains("maven")
            || from.contains("maven")
            || host_of(from) == Some("libraries.minecraft.net")
    }
    // 判据覆盖的规则 id（内置表里就是这 6 条；`bmclapi-liteloader` 的 `from` 不像 maven 仓库，且已因 M225 关闭，不在列）。
    const EXPECTED: [&str; 6] = [
        "bmclapi-libraries",
        "bmclapi-forge-maven",
        "bmclapi-forge-maven-net",
        "bmclapi-neoforge",
        "bmclapi-fabric-maven",
        "bmclapi-maven-central",
    ];
    let mut checked: Vec<&str> = Vec::new();
    for rule in table.rules.iter().filter(|rule| serves_maven(&rule.from)) {
        let prefix = format!("{}{}", rule.to, rule.inject);
        assert!(
            prefix.ends_with("/maven"),
            "规则 {} 的镜像前缀 `{prefix}` 少了 `/maven`：官方 `{}` 在镜像上挂在 `/maven/` 下，\
             丢了它整张表 404（M210/D1）",
            rule.id,
            rule.from
        );
        checked.push(rule.id.as_str());
    }
    // 判据失效（规则被改名/改形/删掉）时这里红：期望的规则一条都不能漏。
    for expected in EXPECTED {
        assert!(
            checked.contains(&expected),
            "库镜像规则 {expected} 没被形态判据覆盖到（被改名/改形/删掉了？）：{checked:?}"
        );
    }
    // 多出来的（新增 maven 规则）也得登记进 EXPECTED，否则说明它绕过了这道判据的意图审查。
    assert_eq!(
        checked.len(),
        EXPECTED.len(),
        "库镜像规则数与预期不符（新增/删除了规则？）：{checked:?}"
    );
}

/// **行为守卫（M210/D1）**：真实库 URL 展开后必须落在镜像的 `/maven/<子路径>` 上。与上一条互为
/// 补充：那条看规则形态，这条看用户实际会请求到的 URL。抓得住：`inject` 丢失、`to` 里手写 `/maven`
/// 被删、或 `from`/`to` 被改成拼不出 `/maven` 的形态；抓不住：只覆盖这 5 个官方 maven 根（各处未必
/// 都写全），镜像侧真实可达性要网络（留给 e2e）。
#[test]
fn library_urls_expand_to_the_maven_subpath_on_bmclapi() {
    let table = MirrorTable::builtin();
    let region = RegionProfile::mirror_first(0);
    let cases = [
        (
            LIBRARIES_URL,
            "https://bmclapi2.bangbang93.com/maven/com/google/guava/guava/32.0.0/guava-32.0.0.jar",
        ),
        (
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar",
            "https://bmclapi2.bangbang93.com/maven/net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar",
        ),
        (
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar",
            "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar",
        ),
        (
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.80/neoforge-21.1.80-installer.jar",
            "https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/21.1.80/neoforge-21.1.80-installer.jar",
        ),
        // M225：LiteLoader 的库来自 Central（`net.minecraft:launchwrapper` 才是 Mojang 库仓）。
        (
            "https://repo1.maven.org/maven2/org/ow2/asm/asm-all/5.2/asm-all-5.2.jar",
            "https://bmclapi2.bangbang93.com/maven/org/ow2/asm/asm-all/5.2/asm-all-5.2.jar",
        ),
    ];
    for (official, expected) in cases {
        let candidates = table.expand(official, ResourceKind::Library, &region);
        let mirror = candidates
            .iter()
            .find(|candidate| candidate.source == "bmclapi")
            .unwrap_or_else(|| panic!("{official} 没有 bmclapi 候选：{candidates:?}"));
        assert_eq!(
            mirror.url, expected,
            "库镜像 URL 形状不对（官方 {official}）"
        );
    }
}

/// **守卫（M225）**：LiteLoader 不套 bmclapi 改写——镜像侧 `versions.json` 与 artifact 实测 404 / 9 B，
/// 官方 200（curl 记录见 `mirrors.json` 的 notes）。抓得住：`bmclapi-liteloader.enabled` 被翻回 true、
/// 或有人给快照仓补来源不明的镜像规则；抓不住：镜像侧哪天真的补上了（那时改期望与 notes，别删测试）。
#[test]
fn liteloader_urls_stay_official_only() {
    let (table, region) = (MirrorTable::builtin(), RegionProfile::mirror_first(0));
    for url in [
        "https://dl.liteloader.com/versions/versions.json",
        "https://dl.liteloader.com/versions/com/mumfrey/liteloader/1.7.10/liteloader-1.7.10_04.jar",
        "https://repo.mumfrey.com/content/repositories/snapshots/com/mumfrey/liteloader/1.12.2-SNAPSHOT/maven-metadata.xml",
    ] {
        let got = table.expand(url, ResourceKind::Library, &region);
        assert_eq!(got.len(), 1, "{url} 不该有镜像候选：{got:?}");
    }
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

    // 「默认前缀」的唯一真相源是**地区档案**（M193：`MirrorTable::set_gh_proxy` / `default_gh_proxy`
    // 已删除——它们改/读的是表里的前缀目录，而 `expand()` 读档案，属于「看起来能改、其实没人读」）。
    assert_eq!(
        RegionProfile::mirror_first(0).gh_proxy.as_deref(),
        Some(unlucky_core::download::region::DEFAULT_GH_PROXY)
    );
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
