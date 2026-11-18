//! 镜像选源（D7 地区策略注入样本）：国内镜像优先 / 国外官方优先，候选里都留兜底。

use unlucky_core::download::mirrors::MirrorTable;
use unlucky_core::download::region::RegionProfile;
use unlucky_core::download::ResourceKind;

#[test]
fn domestic_region_puts_college_mirrors_first() {
    let table = MirrorTable::builtin();
    let url = "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9/OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip";
    let domestic = table.expand(
        url,
        ResourceKind::JavaRuntime,
        &RegionProfile::mirror_first(0),
    );
    assert_eq!(
        domestic.first().map(|candidate| candidate.source.as_str()),
        Some("tuna")
    );
    assert_eq!(
        domestic.first().map(|candidate| candidate.url.as_str()),
        Some("https://mirrors.tuna.tsinghua.edu.cn/Adoptium/21/jdk/x64/windows/OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip")
    );
    let sources: Vec<&str> = domestic
        .iter()
        .map(|candidate| candidate.source.as_str())
        .collect();
    // USTC/SJTU/BFSU 没有这套目录树（实测 404）、华为云是门户假阳性（M34 实测）
    // → 清单里全停用，不该出现在候选里。
    for dead in ["ustc", "sjtu", "bfsu", "huaweicloud"] {
        assert!(!sources.contains(&dead), "{dead} 应已停用：{sources:?}");
    }
    assert_eq!(sources.last().copied(), Some("official"), "官方兜底在最后");
}

#[test]
fn foreign_region_keeps_official_first_but_still_has_mirrors() {
    let table = MirrorTable::builtin();
    let url = "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.13%2B11/OpenJDK17U-jdk_aarch64_linux_hotspot_17.0.13_11.tar.gz";
    let abroad = table.expand(url, ResourceKind::JavaRuntime, &RegionProfile::direct(0));
    assert_eq!(
        abroad.first().map(|candidate| candidate.source.as_str()),
        Some("official")
    );
    assert_eq!(
        abroad.first().map(|candidate| candidate.url.as_str()),
        Some(url)
    );
    // 镜像仍然排在后面（国外也可能是镜像更快；健康度会让它上位）。
    assert!(abroad.len() > 1);
    assert!(abroad
        .iter()
        .any(|candidate| candidate.url.contains("/Adoptium/17/jdk/aarch64/linux/")));
}

#[test]
fn vendor_cdn_and_mirror_urls_are_labelled_by_their_real_host() {
    let table = MirrorTable::builtin();
    let direct = RegionProfile::direct(0);
    // Oracle 走国内镜像时，直连 URL 就是华为云——不能标成 official（否则用户以为在啃官方 5.3KB/s）。
    let oracle =
        "https://mirrors.huaweicloud.com/openjdk/21.0.2/openjdk-21.0.2_windows-x64_bin.zip";
    assert_eq!(
        table
            .expand(oracle, ResourceKind::JavaRuntime, &direct)
            .first()
            .map(|candidate| candidate.source.as_str()),
        Some("huawei")
    );
    // MS 的 aka.ms 与 Zulu 的 CDN 同理。
    let microsoft = "https://aka.ms/download-jdk/microsoft-jdk-21.0.13.0.2-windows-x64.zip";
    assert_eq!(
        table
            .expand(microsoft, ResourceKind::JavaRuntime, &direct)
            .first()
            .map(|candidate| candidate.source.as_str()),
        Some("microsoft")
    );
    let zulu = "https://cdn.azul.com/zulu/bin/zulu21.54.109-ca-jdk21.0.13.0.3-win_x64.zip";
    assert_eq!(
        table
            .expand(zulu, ResourceKind::JavaRuntime, &direct)
            .first()
            .map(|candidate| candidate.source.as_str()),
        Some("azul")
    );
    // Mojang 系资源仍是 official（别把官方源误标成厂商）。
    let mojang = "https://piston-data.mojang.com/v1/objects/abc/client.jar";
    assert_eq!(
        table
            .expand(mojang, ResourceKind::ClientJar, &direct)
            .first()
            .map(|candidate| candidate.source.as_str()),
        Some("official")
    );
}
