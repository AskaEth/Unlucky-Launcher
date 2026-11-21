//! 读 installer jar（zip）里的 `install_profile.json` 与内嵌的版本 JSON。
//!
//! 与 [`super::meta`] 的分工：那边管**网络取数**（把 installer 落到 `.ULL/cache/meta/`），
//! 这里只管**读盘里那只 jar**——纯 IO + 解析，不发请求、不写盘。

use std::fs::File;
use std::path::Path;

use zip::ZipArchive;

use super::forge::InstallProfile;
use super::LoaderError;
use crate::game::manifest::VersionJson;

/// 读 installer jar 里的 `install_profile.json` 与内嵌的版本 JSON。
///
/// 内嵌版本 JSON 的路径来自 profile 的 `json` 字段（通常 `/version.json`）；
/// 该条目不存在时返回 `None`——老协议（1.12.2 之前的大量老重建版）本来就没有这个条目，
/// 版本主体在 profile 内联的 `versionInfo` 里，由 [`super::forge::build_release`] 回退去读。
///
/// # Errors
///
/// 不是合法 zip、缺 `install_profile.json`、profile/版本 JSON 无法解析时返回 metadata 错误。
pub fn read_installer(path: &Path) -> Result<(InstallProfile, Option<VersionJson>), LoaderError> {
    let file = File::open(path).map_err(io_error(path))?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        LoaderError::metadata(format!("{} 不是合法 zip：{error}", path.display()))
    })?;

    let profile_text = read_entry(&mut archive, "install_profile.json")?.ok_or_else(|| {
        LoaderError::metadata(format!(
            "{} 里没有 install_profile.json（不是 Forge/NeoForge installer？）",
            path.display()
        ))
    })?;
    let profile: InstallProfile = serde_json::from_str(&profile_text).map_err(|error| {
        LoaderError::metadata(format!("install_profile.json 解析失败：{error}"))
    })?;

    let embedded = match profile
        .version_json_path
        .as_deref()
        .map(|entry| entry.trim_start_matches('/'))
        .filter(|entry| !entry.is_empty())
    {
        Some(entry) => match read_entry(&mut archive, entry)? {
            Some(text) => Some(VersionJson::from_json_str(&text).map_err(|error| {
                LoaderError::metadata(format!("installer 内 {entry} 解析失败：{error}"))
            })?),
            None => None,
        },
        None => None,
    };
    Ok((profile, embedded))
}

/// 读一个 zip 条目成字符串；条目不存在返回 `Ok(None)`。
fn read_entry(archive: &mut ZipArchive<File>, name: &str) -> Result<Option<String>, LoaderError> {
    use std::io::Read;

    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => {
            return Err(LoaderError::metadata(format!(
                "读 installer 条目 {name} 失败：{error}"
            )))
        }
    };
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|error| LoaderError::io(format!("installer 条目 {name} 不是可读文本：{error}")))?;
    Ok(Some(text))
}

/// 把 `std::io::Error` 包成带路径的 [`LoaderError`]（读缓存目录里的 installer 也会用到）。
pub(super) fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> LoaderError + '_ {
    move |error| LoaderError::io(format!("{} 读写失败：{error}", path.display()))
}
