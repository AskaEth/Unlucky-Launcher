//! 本体构件（forge 的 universal jar）的下载地址补全。
//!
//! 与 [`super::forge`] 的分工：那边组版本 JSON 与判定路线，这里只管「库里哪条是本体的、
//! 它的远端地址该写成什么」。纯计算，不碰网络与磁盘。
//!
//! # 为什么必须补 `-universal`
//!
//! 官方 maven 对 forge 本体**只发 `-universal` classifier**：installer 内嵌的条目叫
//! `maven/…/forge-<ver>.jar`（无 classifier），但官方 maven 上这条路径 **404**
//! （实测 1.12.2-14.23.5.2860：`-universal.jar` 200 / 4466148 字节，与 version.json 声明的
//! sha1 一致；无 classifier 路径仅靠 BMCLAPI 的 302 别名兜底）。
//! 因此这里**只改 URL**：`artifact.path`（磁盘落点）保持官方 installer 的原样，
//! 启动 classpath 布局与官方一致。
//!
//! 两种形状都要覆盖：**形状 A**（spec 0，installer 里有 `version.json`）的本体库有
//! `downloads.artifact`、只是 `url` 是空串；**形状 B**（1.12.2 之前的老重建版）的库表连
//! `downloads` 都没有，见 [`fill_body_artifact`]。

use std::collections::BTreeMap;

use crate::game::library::{Library, LibraryDownloads};
use crate::game::manifest::DownloadInfo;

/// 给一层库表补下载地址（[`super::forge::build_release`] 在返回前调用）。
///
/// - 没有 `url` 的库补 maven 基址（`base` 由加载器种类决定）；
/// - **本体**构件的远端路径追加 `-universal`；
/// - 本体库连 `downloads` 都没有时（形状 B）现补一条 artifact。
pub(super) fn fill_missing_urls(
    libraries: &mut [Library],
    base: &str,
    installer_body: Option<&str>,
) {
    for library in libraries.iter_mut() {
        fill_library(library, base, installer_body);
    }
}

fn fill_library(library: &mut Library, base: &str, installer_body: Option<&str>) {
    // 先算"是不是 installer 声明的本体"（要读 library 的坐标），再拿 artifact 的可变借用。
    let is_body = is_installer_body(library, installer_body);
    if library.downloads.is_none() {
        // 形状 B（老协议）的库表长这样：只有 `name` + 可选的 `url`，**连 `downloads` 都没有**。
        // 本体现补一条 artifact（[`fill_body_artifact`]）；其余这些库（`launchwrapper` /
        // `lzma` / `guava`…）是 **Mojang 的库**：它们住在 `libraries.minecraft.net`，
        // 补成 forge maven 会 404（实测 launchwrapper-1.12 与 lzma-0.0.1 在 forge maven 404、
        // 在 Mojang 库仓 200），所以它们**不写 url**，由 `libraries::plan` 走
        // `DEFAULT_MAVEN_BASE`。
        if is_body {
            fill_missing_url(library, base);
            let root = library.url.clone().unwrap_or_else(|| base.to_string());
            fill_body_artifact(library, &root);
        }
        return;
    }
    fill_missing_url(library, base);
    let root = library.url.clone().unwrap_or_else(|| base.to_string());
    let Some(artifact) = library
        .downloads
        .as_mut()
        .and_then(|downloads| downloads.artifact.as_mut())
    else {
        return;
    };
    if artifact.has_url() {
        return;
    }
    let Some(path) = artifact.path.clone() else {
        return;
    };
    let remote_path = if is_body {
        with_universal_classifier(&path)
    } else {
        path
    };
    artifact.url = Some(join_url(&root, &remote_path));
}

/// 库级 `url` 为空时补加载器自己的 maven 根（凭空靠 `libraries` 的默认仓会拼错）。
fn fill_missing_url(library: &mut Library, base: &str) {
    if library
        .url
        .as_deref()
        .is_none_or(|url| url.trim().is_empty())
    {
        library.url = Some(if base.ends_with('/') {
            base.to_string()
        } else {
            format!("{base}/")
        });
    }
}

/// 形状 B（老协议）的本体：库表里只有 `name` + `url`，**没有 `downloads`**。
///
/// 补一条 artifact：坐标按 maven 规则展开（落点不带 classifier），远端取 `-universal`
/// （官方 maven 只发这个 classifier；与 installer 内嵌那只 jar 是同一个 sha1，实测 1.10.2）。
/// sha1/size 老协议的库表里没有（只有一组含义不明的 `checksums`），所以这条路**只做存在性校验**。
fn fill_body_artifact(library: &mut Library, root: &str) {
    let Ok(coordinate) = library.coordinate() else {
        return;
    };
    let path = coordinate
        .relative_path()
        .to_string_lossy()
        .replace('\\', "/");
    library.downloads = Some(LibraryDownloads {
        artifact: Some(DownloadInfo {
            id: None,
            sha1: None,
            size: None,
            url: Some(join_url(root, &with_universal_classifier(&path))),
            path: Some(path),
        }),
        classifiers: BTreeMap::new(),
    });
}

/// `<root>` + `<相对路径>`（root 尾斜杠可有可无）。
fn join_url(root: &str, path: &str) -> String {
    if root.ends_with('/') {
        format!("{root}{path}")
    } else {
        format!("{root}/{path}")
    }
}

/// 这条库是不是 install profile 的 `path` / `install.path` 声明的本体构件
/// （`group:artifact:version`，无 classifier）。
fn is_installer_body(library: &Library, installer_body: Option<&str>) -> bool {
    let Some(body) = installer_body else {
        return false;
    };
    let mut parts = body.split(':');
    let (Some(group), Some(artifact)) = (parts.next(), parts.next()) else {
        return false;
    };
    library.coordinate().is_ok_and(|coordinate| {
        coordinate.classifier.is_none()
            && coordinate.group == group
            && coordinate.artifact == artifact
    })
}

/// `…/forge-1.12.2-14.23.5.2860.jar` → `…/forge-1.12.2-14.23.5.2860-universal.jar`。
fn with_universal_classifier(path: &str) -> String {
    match path.strip_suffix(".jar") {
        Some(stem) => format!("{stem}-universal.jar"),
        None => path.to_string(),
    }
}
