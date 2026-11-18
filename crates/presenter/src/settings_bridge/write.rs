//! 设置桥的**唯一写入口**（M174 r2 复审 P2-1）。
//!
//! ## 为什么要把「写」收敛成一处
//!
//! [`SettingsBridge`] 的 store 是**长生命**的（启动时 `SettingStore::load` 一次，之后只读），
//! 而 `SettingStore::save` 写的是**整份文档**（`core::setting::store` 的 `write_document`
//! 序列化 `doc.to_value()`，不是脏字段补丁）。于是任何「拿旧快照直接 save」的入口都会把别人
//! 刚写进文件的键整片抹掉：
//!
//! | 轮次 | 入口 | 现象 |
//! |---|---|---|
//! | r1 | [`SettingsBridge::persist`]（外观/主题） | 保存 AI 配置后 1 秒内改主题 ⇒ `ai.*` 连密钥一起消失 |
//! | r2 | `persist_ui_language`（界面语言） | 保存 AI 配置后 1 秒内在同一页切成 English ⇒ 同样消失，而且函数**返回 true**（静默） |
//!
//! 两轮都是「同一类缺陷隔一个文件复发一次」。所以写这件事只留 [`SettingsBridge::write_with`]
//! 一个出口：**现读 → 改 → 落盘 → 刷 mtime**，新入口只能传闭包，忘不了 `reload()`。
//!
//! ## 为什么返回 [`WriteOutcome`] 而不是 `bool`
//!
//! 「没写」有三种原因（内存态 / 没有改动 / 被 schema 只读闸门拦下 / 写盘失败），调用方要能分清：
//! 语言入口要回答「真的落盘了吗」（旧版把 `Skipped` 也当成功），诊断要能说出原因。

use unlucky_core::setting::store::{SaveOutcome, SettingStore, SkipReason};

use super::{mtime, SettingsBridge};

/// 闭包对文档做的事。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edit {
    /// 文档改过了（需要落盘）。
    Changed,
    /// 没有可改的（幂等路径）：不落盘，也不算失败。
    Unchanged,
    /// 改不动（`store.set` 被 schema / 只读闸门拒绝）。
    Rejected,
}

/// 一次写的结果（`Skipped` 的载荷给诊断用；界面只关心 [`WriteOutcome::is_saved`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteOutcome {
    /// 数据根解析不出来（内存态桥）：什么都没写。
    NoStorage,
    /// 闭包说没有改动：没落盘。
    Unchanged,
    /// 真的写进磁盘了。
    Saved,
    /// `save()` 跳过了（只读 / 不可信文件 / 没有脏键）。
    Skipped(SkipReason),
    /// 写盘报错（序列化 / 备份 / IO）。
    Failed,
}

impl WriteOutcome {
    /// 磁盘上真的有新内容吗（**只有这一档算成功**）。
    #[must_use]
    pub(crate) const fn is_saved(self) -> bool {
        matches!(self, Self::Saved)
    }
}

impl SettingsBridge {
    /// 长生命 store 的**唯一写入口**：现读 → 改 → 落盘 → 刷 mtime。
    ///
    /// `edit` 拿到的 `&mut SettingStore` **已经是刚从磁盘重读的那一份**（这一句就是 r1/r2 两个
    /// 缺陷的修法：任何入口都不许拿启动时的旧快照整份覆盖）。
    pub(crate) fn write_with<F>(&mut self, edit: F) -> WriteOutcome
    where
        F: FnOnce(&mut SettingStore) -> Edit,
    {
        let Some(store) = self.store.as_mut() else {
            return WriteOutcome::NoStorage;
        };
        // ← 这一行是两轮复审的修复本体。
        store.reload();
        match edit(store) {
            Edit::Changed => {}
            Edit::Unchanged => return WriteOutcome::Unchanged,
            Edit::Rejected => return WriteOutcome::Failed,
        }
        let outcome = match store.save() {
            Ok(SaveOutcome::Saved(_)) => WriteOutcome::Saved,
            Ok(SaveOutcome::Skipped(reason)) => WriteOutcome::Skipped(reason),
            Err(_) => WriteOutcome::Failed,
        };
        if let Some(path) = self.path() {
            self.last_mtime = mtime(path);
        }
        outcome
    }
}
