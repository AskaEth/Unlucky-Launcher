//! r3 复审 p2：**生产接线**与**降级**的守卫（`super` 按主题分卷）。
//!
//! 这一卷的用例驱动的是生产那个 `collect`（不是它的复制品）：假下载器 + 假取名字器，
//! 于是「失败路径真的去取了不兼容档的名字」这件事在没有网络的用例里也能钉住。

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use unlucky_core::addon::online::{
    ArtifactFetcher, FetchArgs, OnlineError, OnlineErrorKind, OnlineFuture,
};

use super::plan_with_two_tier_failures;
use crate::dto::{
    ContentInstallRequest, ContentSource, InstallFailureKind, InstallProgressView, LoaderKind,
};
use crate::host::addon::install::{collect, Placement, Ports};
use crate::host::addon::plan::Root;

/// 假下载器：一律「写一行字节成功」——本组用例只关心收口那一半（下载阶段另有自己的用例）。
struct QuietFetcher;

impl ArtifactFetcher for QuietFetcher {
    fn fetch<'a>(&'a self, args: FetchArgs<'a>) -> OnlineFuture<'a, Result<u64, OnlineError>> {
        Box::pin(async move {
            std::fs::write(args.dest, b"jar").map_err(|error| {
                OnlineError::new(OnlineErrorKind::Io, format!("写盘失败：{error}"))
            })?;
            Ok(4)
        })
    }
}

fn request_for(instance: &str) -> ContentInstallRequest {
    ContentInstallRequest {
        project: "AANobbMI".to_string(),
        title: "Sodium".to_string(),
        kind: crate::dto::ContentKind::Mod,
        source: ContentSource::Modrinth,
        instance: instance.to_string(),
        game_version: "1.20.1".to_string(),
        loader: LoaderKind::Fabric,
        retry: false,
    }
}

/// **接线层守卫**（r3 复审 p2 第 1 条）：安装失败路径**真的**去取了不兼容档的名字。
///
/// 这条驱动的是生产那个 `collect`（不是它的复制品）：假下载器 + 假取名字器，
/// 于是一份「一个文件下成功 + 一个不兼容前置 + 一条 404」的计划跑完后：
///
/// - 不兼容那条**拿到了名字**（`Entity Culling`）；
/// - 取名字器只被**不兼容**那条问过一次（404 那条不发请求）。
///
/// 变异守门：把 `collect` 里那行 `names_for_incompatible(...)` 删掉/短路（传空名单）⇒ 红。
#[test]
fn the_failed_install_path_really_fetches_names_for_the_incompatible_items() {
    let root = std::env::temp_dir().join(format!("ull-m191-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join(".minecraft/mods");
    std::fs::create_dir_all(&dir).expect("建目录");
    let env = crate::host::HostEnv::at_root(&root);

    let plan = plan_with_two_tier_failures();
    let asked: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let title = |id: &str| {
        asked.lock().expect("锁没中毒").push(id.to_string());
        (id == "P7dR8mSH").then(|| "Entity Culling".to_string())
    };
    let fetcher = QuietFetcher;
    let ports = Ports {
        env: &env,
        fetcher: &fetcher,
        title: &title,
    };
    let (tx, _rx) = crate::host::job::channel();
    let cancel = AtomicBool::new(false);
    let mut progress = InstallProgressView::new();
    let request = request_for("survival");

    let result = collect(
        &ports,
        &request,
        plan,
        Placement {
            dir: &dir,
            root: Root {
                id: "AANobbMI",
                title: "Sodium",
            },
        },
        &cancel,
        &mut progress,
        &tx,
        1,
    );

    assert!(result.is_err(), "有失败项就该回一条失败");
    let incompatible = progress
        .retry
        .iter()
        .find(|item| item.kind == InstallFailureKind::Incompatible)
        .expect("不兼容档要在名单里");
    assert_eq!(
        incompatible.line(),
        "Entity Culling (P7dR8mSH)",
        "失败路径没有把取到的名字贴上去"
    );
    assert_eq!(
        *asked.lock().expect("锁没中毒"),
        vec!["P7dR8mSH".to_string()],
        "只该给不兼容档取名字（404 那条不发请求）"
    );
    let _ = std::fs::remove_dir_all(&root);
}
