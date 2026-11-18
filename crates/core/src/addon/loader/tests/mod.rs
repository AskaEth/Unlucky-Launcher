//! `addon::loader` 的单元测试：离线夹具对拍 + 镜像选择 + unsupported 判定。
//!
//! 刻意**不打真实网络**：JSON 归一化、版本表挑选、镜像候选、计划形状全是纯计算。
//! 夹具是官方响应的**裁剪版**（保留形状与关键字段，去掉几百行库与 base64 图标）。

mod fabric;
mod forge;
mod forge_legacy;
mod liteloader;
mod maven;
mod plan;
mod vocabulary;

/// 404 不许被归成网络故障（M330：1.7.10 双后缀的 404 曾被报成 `loader_network`，
/// 用户被叫去查网络，而网络是好的）。判据走 `effective_kind()`：候选源外循环把
/// 「全 404」汇总成 `AllSourcesFailed + leaf=NotFound`，**只看 `kind` 会把它当网络错**。
#[test]
fn a_404_from_the_download_layer_is_not_a_network_failure() {
    use crate::addon::loader::{LoaderError, LoaderErrorKind};
    use crate::download::{DownloadError, DownloadErrorKind};

    let single = LoaderError::from(DownloadError::new(DownloadErrorKind::NotFound, "404"));
    assert_eq!(single.kind(), LoaderErrorKind::NotFound);
    assert_eq!(single.kind().as_str(), "loader_not_found");

    let all_404 = LoaderError::from(
        DownloadError::new(DownloadErrorKind::AllSourcesFailed, "2 个候选源全部失败")
            .with_leaf(DownloadErrorKind::NotFound),
    );
    assert_eq!(
        all_404.kind(),
        LoaderErrorKind::NotFound,
        "全 404 的汇总错要看叶子：这是坐标不存在，不是网络故障"
    );

    // 对照：真·网络失败仍归 `loader_network`，别把两者的映射写反。
    let network = LoaderError::from(
        DownloadError::new(DownloadErrorKind::AllSourcesFailed, "2 个候选源全部失败")
            .with_leaf(DownloadErrorKind::Network),
    );
    assert_eq!(network.kind(), LoaderErrorKind::Network);
    let timeout = LoaderError::from(DownloadError::new(DownloadErrorKind::Timeout, "超时"));
    assert_eq!(timeout.kind(), LoaderErrorKind::Network);
}

/// 裁剪过的 Fabric `/profile/json`（1.20.1 + fabric-loader 0.15.11）。
pub(crate) const FABRIC_PROFILE: &str = r#"{
  "id": "fabric-loader-0.15.11-1.20.1",
  "inheritsFrom": "1.20.1",
  "releaseTime": "2026-10-29T10:25:18+0000",
  "time": "2026-10-29T10:25:18+0000",
  "type": "release",
  "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
  "arguments": {"game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "]},
  "libraries": [
    {"name": "org.ow2.asm:asm:9.6", "url": "https://maven.fabricmc.net/",
     "sha1": "aa205cf0a06dbd8e04ece91c0b37c3f5d567546a", "size": 123598},
    {"name": "net.fabricmc:sponge-mixin:0.13.3+mixin.0.8.5", "url": "https://maven.fabricmc.net/"},
    {"name": "net.fabricmc:intermediary:1.20.1", "url": "https://maven.fabricmc.net/"},
    {"name": "net.fabricmc:fabric-loader:0.15.11", "url": "https://maven.fabricmc.net/"}
  ]
}"#;

/// 裁剪过的 Quilt `/profile/json`（1.20.1 + quilt-loader 0.20.0-beta.9）。
pub(crate) const QUILT_PROFILE: &str = r#"{
  "id": "quilt-loader-0.20.0-beta.9-1.20.1",
  "inheritsFrom": "1.20.1",
  "type": "release",
  "mainClass": "org.quiltmc.loader.impl.launch.knot.KnotClient",
  "arguments": {"game": []},
  "libraries": [
    {"name": "net.fabricmc:tiny-mappings-parser:0.3.0+build.17", "url": "https://maven.fabricmc.net/"},
    {"name": "org.quiltmc:quilt-json5:1.0.4+final", "url": "https://maven.quiltmc.org/repository/release/"},
    {"name": "org.quiltmc:quilt-loader:0.20.0-beta.9", "url": "https://maven.quiltmc.org/repository/release/"}
  ]
}"#;

/// 原版 1.20.1 的**已裁剪**版本 JSON（继承链解析的父层）。
pub(crate) const VANILLA_1201: &str = r#"{
  "id": "1.20.1",
  "type": "release",
  "mainClass": "net.minecraft.client.main.Main",
  "assets": "5",
  "assetIndex": {"id": "5", "sha1": "0000", "size": 1,
                 "url": "https://piston-meta.mojang.com/v1/packages/5.json"},
  "downloads": {"client": {"sha1": "1111", "size": 2,
                           "url": "https://piston-data.mojang.com/v1/objects/1/client.jar"}},
  "libraries": [
    {"name": "com.mojang:brigadier:1.0.18", "url": "https://libraries.minecraft.net/"}
  ]
}"#;
