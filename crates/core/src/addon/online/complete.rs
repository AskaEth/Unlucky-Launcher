//! 按摘要认文件：[`identity_of`] 判断「站点返回的这个版本里，哪个文件才是我们要的那个」。
//!
//! # 为什么只留这一件
//!
//! 生产上的补全循环只有**一处**：`crate::addon::import::complete`（它走 `PackFetcher`，
//! 因此能区分「站点不认得这个摘要（404）」与「没联网（Offline）」，这决定了要不要提前收工）。
//! 这里曾经还有一份 `complete_files` 循环，r2 复审指出「看着像生产代码其实没人调、两套循环
//! 迟早漂移」⇒ 已删除。本文件只留**判定**，两个调用点共用：
//!
//! | 调用方 | 用它做什么 |
//! |--------|------------|
//! | `addon::import::fetch`（生产补全） | 按 sha512 反查回来的版本 → 取出那份文件的官方 sha1/大小/直链 |
//! | `online::complete` 的用例 | 钉住「对不上就不许用」 |
//!
//! # 一致性比「有值」重要
//!
//! 站点返回的版本里可能有多个文件（`primary` 之外还有 `sources.jar`）。只在
//! **`hashes.sha512` 与请求值逐字相同**的那条上取 `sha1`；一个都对不上就返回 `None`
//! ——宁可让上游记「补不出」，也不把别的东西的摘要装进来。

use super::model::Version;

/// 从版本里找出**摘要对得上**的那个文件，取出它的 sha1/大小/地址。
///
/// `None` = 这个版本里没有任何文件的 sha512 与请求值相同（站点数据不一致 / 版本被换过），
/// 或者对上的那条没有 sha1 可用。
#[must_use]
pub fn identity_of(version: &Version, sha512: &str) -> Option<Identity> {
    let file = version.files.iter().find(|file| {
        file.sha512()
            .is_some_and(|hex| hex.eq_ignore_ascii_case(sha512))
    })?;
    let sha1 = file.sha1().map(str::to_string)?;
    Some(Identity {
        sha1,
        size: (file.size > 0).then_some(file.size),
        url: (!file.url.trim().is_empty()).then(|| file.url.clone()),
    })
}

/// 补全出来的一份身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// 站点给的 SHA-1
    pub sha1: String,
    /// 站点给的字节数（`0` = 没给）
    pub size: Option<u64>,
    /// 站点给的直链（`None` = 也没给）
    pub url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::identity_of;
    use crate::addon::online::model::{Version, VersionFile};

    fn version(files: Vec<VersionFile>) -> Version {
        Version {
            id: "v1".to_string(),
            project_id: "p1".to_string(),
            files,
            ..Version::default()
        }
    }

    fn file(sha1: Option<&str>, sha512: &str, primary: bool) -> VersionFile {
        let mut file = VersionFile {
            url: "https://cdn.modrinth.com/data/p1/v1/a.jar".to_string(),
            filename: "a.jar".to_string(),
            size: 2048,
            primary,
            ..VersionFile::default()
        };
        file.hashes.insert("sha512".to_string(), sha512.to_string());
        if let Some(sha1) = sha1 {
            file.hashes.insert("sha1".to_string(), sha1.to_string());
        }
        file
    }

    #[test]
    fn the_matching_file_wins_even_when_it_is_not_the_primary() {
        let wanted = "a".repeat(128);
        let other = "b".repeat(128);
        let version = version(vec![
            file(Some(&"1".repeat(40)), &other, true),
            file(Some(&"2".repeat(40)), &wanted, false),
        ]);
        let identity = identity_of(&version, &wanted).expect("摘要对得上，必须给出身份");
        assert_eq!(identity.sha1, "2".repeat(40));
        assert_eq!(identity.size, Some(2048));
        assert!(identity.url.is_some());
    }

    #[test]
    fn a_version_without_that_hash_yields_nothing() {
        let version = version(vec![file(Some(&"1".repeat(40)), &"b".repeat(128), true)]);
        assert_eq!(identity_of(&version, &"c".repeat(128)), None);
    }

    #[test]
    fn a_matching_file_without_a_sha1_is_not_usable() {
        // 站点只给了 sha512：这时没有可填的 sha1，宁可「补不出」也不许拿 sha512 冒充。
        let wanted = "a".repeat(128);
        let version = version(vec![file(None, &wanted, true)]);
        assert_eq!(identity_of(&version, &wanted), None);
    }

    #[test]
    fn hex_case_does_not_matter() {
        let wanted = "AbCd".repeat(32);
        let version = version(vec![file(
            Some(&"1".repeat(40)),
            &wanted.to_ascii_lowercase(),
            true,
        )]);
        assert!(identity_of(&version, &wanted).is_some());
    }
}
