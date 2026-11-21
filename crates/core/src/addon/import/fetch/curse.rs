//! CurseForge 条目补全的真实实现（M317）：`projectID/fileID` → 文件名 / 地址 / 摘要 / 类别。
//!
//! # 为什么要两次 GET
//!
//! 站点把 `classId` **只挂在 mod 上**（`GET /mods/{id}`），文件端点
//! （`GET /mods/{id}/files/{fid}`）不带它；而导入侧要靠 `classId` 决定落盘目录
//! （`plan::curse_class_dir`）。路径靠猜就会把资源包装进 `mods/`，
//! 所以宁可多一次只读 GET：**一次条目两条请求**是这条路的成本下限，不做指纹反查、不拉整份
//! 文件列表（那会多出几十倍配额）。
//!
//! 顺序是「先文件、再项目」：文件是主体（名字/地址/摘要），它失败就没必要再花一次配额。
//!
//! # 错误映射（本文件的 `match` 是唯一一处）
//!
//! | 站点/客户端怎么说 | [`FetchErrorKind`] | 用户该做什么 |
//! |-------------------|--------------------|--------------|
//! | 本地没配 Key（一次请求都没发） | [`FetchErrorKind::NotConfigured`] | 去设置里填自己的 Key |
//! | 404 / 410 | [`FetchErrorKind::NotFound`] | 这条确实没了 |
//! | 429 / 5xx / 连不上 | [`FetchErrorKind::Network`] | 过一会儿再导入 |
//! | 401 / 403（Key 无效） | [`FetchErrorKind::Unsupported`] | 换一把 Key |
//! | 响应不是期望形状 / 没有可用地址 | [`FetchErrorKind::Unsupported`] | 无（我们或站点那边的事） |
//!
//! 全部落在「可容忍」一侧（[`FetchErrorKind::is_tolerable`]）：一条补不出来只让**这一条**记为
//! 缺文件，整包导入仍是 `Partial`。**「缺 Key」与「这条真装不了」必须分得开**——前者去填就能
//! 干，后者填了也没用，所以 `NotConfigured` 是一个独立档，不复用 `Unsupported`。
//!
//! # 密钥纪律
//!
//! Key 只在 [`crate::addon::curseforge::CurseForge`] 的请求拼装点取值（`ApiKey::expose`），
//! 本文件只把 `&ApiKey` 递进去；下面每一条错误只带状态码与站点原因，**Key 不进来**。

use crate::addon::curseforge::{
    ApiKey, CfError, CfErrorKind, CfTransport, CurseForge, RetryPolicy,
};

use super::{CurseFileMeta, FetchError, FetchErrorKind};

/// 「有 Key 就能干」的具名状态（默认实现与没配 Key 的生产实现共用一份措辞）。
pub(crate) fn key_missing(project_id: i64, file_id: i64) -> FetchError {
    FetchError::new(
        FetchErrorKind::NotConfigured,
        format!(
            "CurseForge 需要用户自带的 API Key（x-api-key），当前没有配置；本条记为缺文件，其余条目照装（projectID={project_id} fileID={file_id}）"
        ),
    )
}

/// 补全一条 CF 条目（离线可测：`transport` 是注入的，本函数不碰 `reqwest`）。
pub(crate) async fn curse_meta<T: CfTransport + ?Sized>(
    transport: &T,
    key: Option<&ApiKey>,
    policy: RetryPolicy,
    project_id: i64,
    file_id: i64,
) -> Result<CurseFileMeta, FetchError> {
    let (Ok(project), Ok(file)) = (u32::try_from(project_id), u32::try_from(file_id)) else {
        // 负数 / 超 u32 的 id 只可能来自坏清单：这条不值得花配额，如实记缺。
        return Err(FetchError::new(
            FetchErrorKind::Unsupported,
            format!("CurseForge 的 id 不是正整数（projectID={project_id} fileID={file_id}），本条记为缺文件"),
        ));
    };
    let Some(key) = key else {
        return Err(key_missing(project_id, file_id));
    };
    let client = CurseForge::with_key(transport, key).with_policy(policy);

    let meta = client.file(project, file).await.map_err(FetchError::from)?;
    // 有 `downloadUrl` 就用它，`None`（作者关了 API 分发）才问 `/download-url`：两步都不编地址。
    let url = client.download_url(&meta).await.map_err(FetchError::from)?;
    let file_name = meta.file_name.trim().to_string();
    if file_name.is_empty() {
        return Err(FetchError::new(
            FetchErrorKind::Unsupported,
            format!("CurseForge 没给文件名（projectID={project} fileID={file}），本条记为缺文件"),
        ));
    }
    // 目录取自项目类别：认不出就不猜（`resolve` 会把 `class_id: None` 记成缺）。
    let class_id = client
        .project_class(project)
        .await
        .map_err(FetchError::from)?;
    Ok(CurseFileMeta {
        file_name,
        url: Some(url),
        // CF 官方只给 sha1（md5 那条不用：引擎与清单都按 sha1 对）。
        sha1: meta.sha1().map(str::to_string),
        // 站点没给长度时是 0：**不**把它当「期望 0 字节」传给下载层。
        size: Some(meta.file_length).filter(|size| *size > 0),
        class_id: class_id.map(i64::from),
    })
}

impl From<CfError> for FetchError {
    fn from(error: CfError) -> Self {
        let kind = match error.kind() {
            CfErrorKind::NotConfigured => FetchErrorKind::NotConfigured,
            CfErrorKind::NotFound => FetchErrorKind::NotFound,
            // 限流 / 站点错 / 网络：这一条暂时搞不到，等下一条或下一次导入。
            CfErrorKind::RateLimited { .. } | CfErrorKind::Server | CfErrorKind::Network => {
                FetchErrorKind::Network
            }
            // Key 无效与「这条真装不了」：都不该让整包失败，但也都不是「去填 Key 就能好」。
            CfErrorKind::InvalidKey | CfErrorKind::Malformed | CfErrorKind::Unsupported => {
                FetchErrorKind::Unsupported
            }
        };
        Self::new(kind, error.message())
    }
}
