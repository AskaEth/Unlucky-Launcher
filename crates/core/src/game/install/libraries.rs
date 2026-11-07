//! 依赖库展开：主构件 + natives 分类器构件 → 待下载文件。
//!
//! 依据：`DevDocs/analyse/03-version-download.md` §6.4（libraries 含 natives 规则）、
//! §6.8（目录布局速查）。
//!
//! 三条容易踩的规则：
//! 1. **主构件与 natives 构件都要下**：老版本（lwjgl 2.x）一条库里同时有
//!    `downloads.artifact` 与 `natives` 表，HMCL 也是两者都取；
//! 2. **路径优先用 JSON 给的 `path`**（Mojang 权威值），缺了才按 Maven 坐标推导；
//! 3. **`hint=local` 的库落到实例私有目录** `versions/<id>/libraries/`，其余一律进
//!    恒共享的 `libraries/`（D20：任何模式都不按实例复制）。

use std::path::{Path, PathBuf};

use super::super::arguments::Environment;
use super::super::layout::GameLayout;
use super::super::library::Library;
use super::plan::{FileRole, PlanFile};
use crate::download::ResourceKind;

/// 没写 `url` 的库兜底用的 Maven 基址（Mojang 官方库仓）。
pub const DEFAULT_MAVEN_BASE: &str = "https://libraries.minecraft.net/";

/// 展开一层（已合并）清单里的库列表。
///
/// 返回（文件清单，人读告警）。告警不致命：缺下载地址的库会被跳过并告知，
/// 让用户知道"缺哪个文件"而不是拿到一个静默不完整的实例。
#[must_use]
pub fn plan(
    layout: &GameLayout,
    version_id: &str,
    libraries: &[Library],
    environment: &Environment,
) -> (Vec<PlanFile>, Vec<String>) {
    let mut files = Vec::new();
    let mut warnings = Vec::new();

    for library in libraries {
        if !library.applies(environment) {
            continue;
        }
        if library.coordinate().is_err() {
            warnings.push(format!("库坐标不合法，已跳过：{}", library.name));
            continue;
        }
        let Ok(relative) = library.relative_path() else {
            warnings.push(format!("库路径推导失败，已跳过：{}", library.name));
            continue;
        };
        let dest = library_dest(layout, version_id, library, &relative);

        match library.artifact() {
            Some(artifact) if artifact.has_url() => files.push(PlanFile {
                role: FileRole::Library,
                name: library.name.clone(),
                url: artifact.url.clone().unwrap_or_default(),
                sha1: artifact.sha1.clone(),
                size: artifact.size,
                dest,
                resource: ResourceKind::Library,
            }),
            // natives-only 库（1.6 时代的 `lwjgl-platform` / `jinput-platform`）**没有主构件**，
            // 只要 `downloads.classifiers`。这里若按坐标硬拼 URL 会拼出一个不存在的 jar
            // （实测：官方与 BMCLAPI 双双 404）。判据是 `natives` 在场。
            None if library.natives.is_some() => {}
            // 魔改/现代 loader 的库常见"只给 name 不给 downloads"，按坐标拼仓库地址。
            _ => files.push(PlanFile {
                role: FileRole::Library,
                name: library.name.clone(),
                url: maven_url(library, &relative),
                sha1: None,
                size: None,
                dest,
                resource: ResourceKind::Library,
            }),
        }

        // natives 分类器构件（存在才下；解压发生在启动期，不归安装管）。
        let Some(classifier) = library.native_classifier(environment) else {
            continue;
        };
        let Some(artifact) = library.native_artifact(&classifier) else {
            warnings.push(format!(
                "{} 的 natives 分类器 {classifier} 没有下载信息，已跳过",
                library.name
            ));
            continue;
        };
        let Ok(native_relative) = library.native_relative_path(&classifier) else {
            warnings.push(format!("{} 的 natives 路径推导失败，已跳过", library.name));
            continue;
        };
        if !artifact.has_url() {
            warnings.push(format!(
                "{} 的 natives 构件 {classifier} 没有下载地址，已跳过",
                library.name
            ));
            continue;
        }
        files.push(PlanFile {
            role: FileRole::Native,
            name: format!("{}:{classifier}", library.name),
            url: artifact.url.clone().unwrap_or_default(),
            sha1: artifact.sha1.clone(),
            size: artifact.size,
            dest: layout.libraries_dir().join(native_relative),
            resource: ResourceKind::Library,
        });
    }

    (files, warnings)
}

/// 库文件落点：实例私有（`hint=local`）或恒共享 `libraries/`。
fn library_dest(
    layout: &GameLayout,
    version_id: &str,
    library: &Library,
    relative: &Path,
) -> PathBuf {
    if library.is_local() {
        layout.version_libraries_dir(version_id).join(relative)
    } else {
        layout.libraries_dir().join(relative)
    }
}

/// 按库声明（或默认）的 Maven 基址拼下载地址。
fn maven_url(library: &Library, relative: &Path) -> String {
    let base = library
        .url
        .as_deref()
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_MAVEN_BASE);
    let path = relative.to_string_lossy().replace('\\', "/");
    if base.ends_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

/// 供上层判错用：库展开不产生硬失败，缺地址一律降级成告警。
#[cfg(test)]
mod tests {
    use super::plan;
    use crate::game::arguments::Environment;
    use crate::game::layout::GameLayout;
    use crate::game::library::Library;

    fn environment() -> Environment {
        Environment {
            os: "windows".to_string(),
            os_version: String::new(),
            arch: "x86_64".to_string(),
            ..Environment::host()
        }
    }

    fn library_with_artifact(json: &str) -> Library {
        serde_json::from_str(json).expect("库 JSON 应可解析")
    }

    #[test]
    fn plans_artifact_at_maven_path() {
        let library = library_with_artifact(
            r#"{"name":"org.ow2.asm:asm:9.6","downloads":{"artifact":{"path":"org/ow2/asm/asm/9.6/asm-9.6.jar","sha1":"aa","size":122,"url":"https://libraries.minecraft.net/org/ow2/asm/asm/9.6/asm-9.6.jar"}}}"#,
        );
        let layout = GameLayout::new("/g/.minecraft");
        let (files, warnings) = plan(&layout, "1.20.1", &[library], &environment());
        assert!(warnings.is_empty());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].sha1.as_deref(), Some("aa"));
        assert_eq!(files[0].size, Some(122));
        assert!(files[0]
            .dest
            .ends_with("libraries/org/ow2/asm/asm/9.6/asm-9.6.jar"));
    }

    #[test]
    fn natives_only_library_plans_no_artifact() {
        // 1.6 时代的 lwjgl-platform / jinput-platform：有 natives 表、只有 classifiers，
        // **没有 downloads.artifact**。按坐标硬拼主构件会拼出不存在的 jar（实测双源 404）。
        let library = library_with_artifact(
            r#"{"name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.0",
                "natives":{"windows":"natives-windows","linux":"natives-linux","osx":"natives-osx"},
                "downloads":{"classifiers":{"natives-windows":{"path":"org/lwjgl/lwjgl/lwjgl-platform/2.9.0/lwjgl-platform-2.9.0-natives-windows.jar","sha1":"3f11","size":609967,"url":"https://libraries.minecraft.net/natives-windows.jar"}}}}"#,
        );
        let layout = GameLayout::new("/g/.minecraft");
        let (files, warnings) = plan(&layout, "1.6.4", &[library], &environment());
        assert!(warnings.is_empty(), "不该有告警：{warnings:?}");
        assert_eq!(files.len(), 1, "只应有 natives 构件：{files:?}");
        assert_eq!(files[0].role.as_str(), "native");
        assert_eq!(files[0].sha1.as_deref(), Some("3f11"));
    }

    #[test]
    fn plans_both_artifact_and_native_jar() {
        let library = library_with_artifact(
            r#"{"name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.4","natives":{"windows":"natives-windows"},
                "downloads":{"artifact":{"path":"org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4.jar","sha1":"bb","size":1,"url":"https://libraries.minecraft.net/a.jar"},
                "classifiers":{"natives-windows":{"path":"org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-windows.jar","sha1":"cc","size":2,"url":"https://libraries.minecraft.net/b.jar"}}}}"#,
        );
        let layout = GameLayout::new("/g/.minecraft");
        let (files, _) = plan(&layout, "1.7.10", &[library], &environment());
        assert_eq!(files.len(), 2);
        let native = files
            .iter()
            .find(|f| f.role.as_str() == "native")
            .expect("应有 natives 构件");
        assert_eq!(native.sha1.as_deref(), Some("cc"));
        assert!(native
            .dest
            .ends_with("lwjgl-platform-2.9.4-natives-windows.jar"));
    }

    #[test]
    fn derives_url_for_libraries_without_downloads() {
        let library = library_with_artifact(
            r#"{"name":"net.minecraftforge:forge:1.20.1-47.2.0:universal","url":"https://maven.minecraftforge.net/"}"#,
        );
        let layout = GameLayout::new("/g/.minecraft");
        let (files, warnings) = plan(&layout, "1.20.1-forge", &[library], &environment());
        assert!(warnings.is_empty());
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].url,
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-universal.jar"
        );
        assert!(files[0].sha1.is_none());
    }

    #[test]
    fn skips_libraries_filtered_by_rules() {
        let library = library_with_artifact(
            r#"{"name":"org.lwjgl:lwjgl:3.3.2","rules":[{"action":"allow","os":{"name":"linux"}}],
                "downloads":{"artifact":{"url":"https://libraries.minecraft.net/x.jar","sha1":"dd"}}}"#,
        );
        let layout = GameLayout::new("/g/.minecraft");
        let (files, _) = plan(&layout, "1.20.1", &[library], &environment());
        assert!(files.is_empty());
    }

    #[test]
    fn local_hint_libraries_land_in_instance_dir() {
        let library = library_with_artifact(
            r#"{"name":"net.minecraftforge:forge:1.20.1-47.2.0","hint":"local",
                "downloads":{"artifact":{"path":"net/minecraftforge/forge/1.20.1-47.2.0.jar","url":"https://libraries.minecraft.net/y.jar"}}}"#,
        );
        let layout = GameLayout::new("/g/.minecraft");
        let (files, _) = plan(&layout, "inst", &[library], &environment());
        assert_eq!(files.len(), 1);
        assert!(files[0]
            .dest
            .ends_with("versions/inst/libraries/net/minecraftforge/forge/1.20.1-47.2.0.jar"));
    }
}
