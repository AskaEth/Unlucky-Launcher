//! 新建实例向导提交给 host 的请求。

use serde::{Deserialize, Serialize};

use super::LoaderKind;

/// 新建实例向导提交给 host 的请求（已通过向导校验）。
///
/// ⚠️ **没有「父版本」槽位**：向导/下载页只装「叠在原版上」的那一层。要把 LiteLoader 叠在
/// Forge 上，core 侧已有真实装配路径（`LoaderSource::resolve_on` 的 `parent` 参数 +
/// `LoaderPlanInput::parent_installed`），但把父版本从 UI 传下来需要同时扩 `CreateRequest`、
/// 下载页草稿与向导校验三处，本轮只做原版父链（见 M222 报告「未做/待排」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRequest {
    /// 目标原版版本 id（`1.20.4`）
    pub version: String,
    /// 加载器
    pub loader: LoaderKind,
    /// 加载器版本（空 = 让 core 取最新）
    pub loader_version: String,
    /// 实例名（空 = 用版本 id / 加载器版本 id）
    pub name: String,
}

impl CreateRequest {
    /// 实际使用的实例名。
    pub fn instance_name(&self) -> String {
        if self.name.trim().is_empty() {
            self.version.clone()
        } else {
            self.name.trim().to_string()
        }
    }
}
