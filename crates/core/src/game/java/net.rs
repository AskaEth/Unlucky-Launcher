//! 版本查询用的极小 HTTP 取文本工具（四家发行版的「一锤子 GET」共用）。
//!
//! 只有一件事：GET → 状态分类 → 整段文本。**下载不走这里**（那归 `core::download` 引擎），
//! 所以没有重试、没有镜像展开、没有进度事件——查清单失败就直接把错误抛给发行版回退链，
//! 让它换下一家（这是 D23「vendor 级失败自动降下一 vendor」的最短路径）。

use super::error::{JavaError, JavaErrorKind};

/// 取一段文本（`what` 是给用户看的名词，例如「华为云 openjdk 目录」）。
///
/// # Errors
///
/// 连不上 / 超时 → [`JavaErrorKind::Network`]；404 或 410 → [`JavaErrorKind::Metadata`]
/// （「这个东西上游没有」比「网络坏了」更准确，回退链据此换 vendor 而不是重试同一条）。
pub async fn get_text(
    client: &reqwest::Client,
    url: &str,
    what: &str,
) -> Result<String, JavaError> {
    let response = client.get(url).send().await.map_err(|error| {
        JavaError::new(
            JavaErrorKind::Network,
            format!("取 {what} 失败：{error}（{url}）"),
        )
    })?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
        return Err(JavaError::new(
            JavaErrorKind::Metadata,
            format!("{what} 不存在（HTTP {status}）：{url}"),
        ));
    }
    if !status.is_success() {
        return Err(JavaError::new(
            JavaErrorKind::Network,
            format!("{what} 返回 HTTP {status}：{url}"),
        ));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = response.text().await.map_err(|error| {
        JavaError::new(
            JavaErrorKind::Network,
            format!("读 {what} 响应失败：{error}"),
        )
    })?;
    // 门户兜底页会把「路径不存在」伪装成 200 + HTML；版本清单页面本身可能也是 HTML，
    // 所以这里不做内容判定，只看调用方能不能解析出来（解析失败会给出更准的原因）。
    if body.trim().is_empty() {
        return Err(JavaError::new(
            JavaErrorKind::Metadata,
            format!("{what} 返回空响应（Content-Type: {content_type}）：{url}"),
        ));
    }
    Ok(body)
}
