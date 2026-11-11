//! 父版本链：把「叠在哪一层上」翻译成原版版本 id（LiteLoader 叠在 Forge 上时要用）。
//!
//! 加载器层只声明 `inheritsFrom`，客户端 jar、资源索引都在**根**那一层（原版）。所以当用户
//! 指定的父版本不是原版（`1.12.2-forge-14.23.5.2860`）时，我们既要记住父版本 id（写进
//! 合成清单的 `inheritsFrom`），又要知道根的原版 id（拼 loader 版本 id、定位 jar）。
//!
//! 判据只有一条：**读盘**。父版本在盘上就必须读得出它的 `inheritsFrom`；读不出来说明父版本
//! 压根没装好，这时报错比猜一个原版版本号强（猜错会装出 `inheritsFrom` 指向不存在版本、
//! 启动必崩的实例）。沿链走最多 [`MAX_DEPTH`] 跳，成环即报错（与
//! [`crate::game::inherit::resolve`] 同口径）。

use crate::game::layout::GameLayout;
use crate::game::manifest::VersionJson;

use super::LoaderError;

/// 继承链深度上限（现实里 loader + patch 不超过 5 层）。
pub const MAX_DEPTH: usize = 32;

/// 从盘上的版本 JSON 沿 `inheritsFrom` 走到根，返回根版本 id（原版）。
///
/// # Errors
///
/// 版本 JSON 不存在 → `not_found`（父版本还没装）；读不出来 / 成环 → `metadata`。
pub fn root_version_id(layout: &GameLayout, version_id: &str) -> Result<String, LoaderError> {
    let mut chain = vec![version_id.to_string()];
    let mut current = version_id.to_string();
    for _ in 0..MAX_DEPTH {
        let manifest = read_manifest(layout, &current)?;
        match manifest.inherits_from.as_deref().map(str::trim) {
            Some(parent) if !parent.is_empty() => {
                if chain.iter().any(|id| id == parent) {
                    chain.push(parent.to_string());
                    return Err(LoaderError::metadata(format!(
                        "父版本链成环：{}",
                        chain.join(" → ")
                    )));
                }
                chain.push(parent.to_string());
                current = parent.to_string();
            }
            _ => return Ok(current),
        }
    }
    Err(LoaderError::metadata(format!(
        "父版本链超过 {MAX_DEPTH} 层，拒绝继续解析：{}",
        chain.join(" → ")
    )))
}

/// 读一份版本 JSON；文件不存在时给出「先装父版本」的可执行提示。
fn read_manifest(layout: &GameLayout, version_id: &str) -> Result<VersionJson, LoaderError> {
    let path = layout.version_json(version_id);
    let text = std::fs::read_to_string(&path).map_err(|error| {
        LoaderError::not_found(format!(
            "父版本 {version_id} 还没装好（读不到 {}：{error}）",
            path.display()
        ))
        .with_hint("先用同一个父版本装一次它自己的加载器（例如 `install <原版> --loader forge`），再在它上面叠")
    })?;
    VersionJson::from_json_str(&text).map_err(|error| {
        LoaderError::metadata(format!("父版本 {version_id} 的清单解析失败：{error}"))
    })
}
