//! 「助手」分组的**真读写**：`config/launcher.json` 的 `ai.enabled` / `ai.provider` /
//! `ai.base_url` / `ai.model` / `ai.api_key` 五键。
//!
//! ## 为什么在进程内直接过 `SettingStore`，不走 `AppCommand::SetSetting`
//!
//! `AppCommand` 是可序列化的 DTO（`dto/events.rs` 的 `derive(Serialize, Deserialize)`），
//! 把 `ai.api_key` 塞进去就等于让明文进了「命令通道」——事件日志、调试转储、mock 后端的事件
//! 回放都会看到它。M174 的守卫①要求密钥**不出进程内**，所以本组写盘走
//! `core::setting::store::SettingStore`：schema 校验 + 原子写照旧（不自己拼 JSON、不绕 schema），
//! 只是不经过那条可序列化的通道。这与 `SettingsBridge::persist` 写外观是同一条做法。
//!
//! ## 为什么每次现读现写
//!
//! `SettingStore::load` 每次从磁盘读一份（`HostEnv::store()` 也是这个口径），所以「读 → 改 → 存」
//! 不会用启动时的旧快照盖掉别人刚写的键（AI / `ull config set` 是同一个文件的外部写者）。

use std::path::Path;

use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::{SaveOutcome, SettingStore};

use super::state::{ProviderId, Snapshot, State};

/// 读一份真值。文件缺失/坏掉时 `SettingStore::load` 自己降级，这里只会读到 schema 默认值。
#[must_use]
pub fn load(path: &Path) -> Snapshot {
    let store = SettingStore::load(path, launcher_schema());
    snapshot(store.doc())
}

/// 把草稿落盘，返回**落盘后**再读回来的真值（界面据此刷新，而不是相信自己的草稿）。
///
/// # Errors
/// schema 拒绝（越界/类型/只读文件）或写盘失败时返回人话（已过 [`super::state::State::validate`]，
/// 所以这里只剩「schema/磁盘」这一类）。
pub fn save(path: &Path, state: &State) -> Result<Snapshot, String> {
    let mut store = SettingStore::load(path, launcher_schema());
    // `Value` 是 `set` 的形参类型：用 `.into()` 让目标类型从签名推出，本 crate 就不必直接依赖
    // `serde_json`（`Cargo.toml` 不在本 mission 的 scope，加依赖要单独走）。
    store
        .set("ai.enabled", state.enabled.into())
        .and_then(|()| store.set("ai.provider", state.provider.setting().to_string().into()))
        .and_then(|()| store.set("ai.base_url", state.base_url.trim().to_string().into()))
        .and_then(|()| store.set("ai.model", state.model.trim().to_string().into()))
        .map_err(|error| format!("{error}"))?;

    // 密钥：草稿非空才写（空 = 保留已存的那一把，界面上写着这条规则）。
    if !state.key.is_empty() {
        store
            .set("ai.api_key", state.key.expose().to_string().into())
            .map_err(|error| format!("{error}"))?;
    }

    // **只有 `SaveOutcome::Saved` 算成功**（M174 r2 复审）：`save()` 还有一档 `Skipped(reason)`
    // ——只读（`$schema` 认不出 / 文件被更新版本的启动器写过）、不可信文件、没有脏键。旧代码
    // 用 `.is_ok()` 把这一档也当成功，于是「什么都没写进磁盘」在界面上是「已保存」，重启回旧值。
    match store.save() {
        Ok(SaveOutcome::Saved(_)) => {}
        Ok(SaveOutcome::Skipped(reason)) => {
            return Err(format!("未写入磁盘（{}）", skip_reason(reason)));
        }
        Err(error) => return Err(format!("{error}")),
    }

    // 落盘之后**真的从磁盘再读一份**：`store.doc()` 是这次改过的内存视图，它证明不了「文件里
    // 现在是什么」（上面那句注释在 r1 就有，代码却没照做——r2 复审按这句判的）。
    let reloaded = SettingStore::load(path, launcher_schema());
    Ok(snapshot(reloaded.doc()))
}

/// `SkipReason` → 人话（`core` 的稳定名对用户没意义）。
fn skip_reason(reason: unlucky_core::setting::store::SkipReason) -> &'static str {
    use unlucky_core::setting::store::SkipReason;
    match reason {
        SkipReason::NotDirty => "没有改动",
        SkipReason::ReadOnly => "配置文件只读：它被更新版本的启动器写过，或被标记为不可信",
        SkipReason::Unreadable => "配置文件不可信（加载时就降级成只读）",
    }
}

/// 文档 → 快照（密钥只取长度，值不出本函数）。
fn snapshot(doc: &unlucky_core::setting::document::SettingDocument) -> Snapshot {
    let text = |key: &str| doc.effective_str(key).unwrap_or_default().to_string();
    let key_chars = doc
        .effective_str("ai.api_key")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or(0, |value| value.chars().count());
    Snapshot {
        enabled: doc.effective_bool("ai.enabled").unwrap_or(true),
        provider: ProviderId::from_setting(&text("ai.provider")),
        base_url: text("ai.base_url"),
        model: text("ai.model"),
        key_chars,
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
