//! 本地 Mod 的快照落地与「请求在飞」标记（M224 · MODTOGGLE）。
//!
//! 从 `state.rs` 拆出（那个文件正好贴着 400 行硬线），形状与 [`super::accounts`] / [`super::live`]
//! 一致：**另一个 `impl AppState` 块**，路径与可见性都不变，`apply_core` 只有一行投递。
//!
//! # 为什么快照住在这里而不是页面
//!
//! 「列表画的永远是磁盘上的真相」是本模块唯一要保证的事，所以：
//!
//! - 快照走 [`crate::dto::CoreEvent::Mods`] 的 `Ok` 一支，**整体替换**（不做增量合并）——host 每次
//!   都是重新扫一遍目录才发的，合并只会两处真相；
//! - 失败走同一个事件的 `Err` 一支，记下一条**具名原因**，页面按 i18n key 现求文案
//!   （presenter 不拼句子，M161 纪律）；
//! - [`ModsState::pending`] 是**页面交互态**（哪个文件的请求在飞），只用来把那个开关先置灰
//!   ——**不做乐观翻面**：`view` 里的 `enabled` 只跟磁盘快照走。
//!
//! # 迟到结果
//!
//! 启停是「后台线程 + 快照回来」：用户很可能点完就切到别的实例。所以 `view.instance` 与
//! [`ModsState::bound`] 不一致的结果**直接丢掉**（与内容库按 request id 判迟到同一条纪律）——
//! 否则 A 的列表会被 B 的启停结果顶掉。

use std::collections::BTreeSet;

use crate::dto::{ModFailureView, ModListView};

use super::AppState;

/// 本地 Mod 的视图态。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModsState {
    /// 当前详情页的实例 id（`None` = 不在详情页）。
    pub bound: Option<String>,
    /// 最近一份快照（`None` = 还没请求过）。
    pub view: Option<ModListView>,
    /// 最近一条失败（具名原因；成功一次就清掉）。
    pub failed: Option<ModFailureView>,
    /// 正在飞的启停请求（磁盘文件名，含后缀）。
    pub pending: BTreeSet<String>,
}

impl ModsState {
    /// 切到另一个实例（换实例必须清空：A 的列表不能画在 B 的详情页上）。
    pub(super) fn bind(&mut self, instance: Option<&str>) {
        if self.bound.as_deref() == instance {
            return;
        }
        self.bound = instance.map(str::to_string);
        self.view = None;
        self.failed = None;
        self.pending.clear();
    }

    /// 一条结果落地（`Ok` = 快照整体替换，`Err` = 记一条具名失败；迟到/串台的丢掉）。
    pub fn apply(&mut self, result: Result<ModListView, ModFailureView>) {
        match result {
            Ok(view) => self.changed(view),
            Err(failure) => self.failed(failure),
        }
    }

    /// 列表快照落地（整体替换；迟到/串台的丢掉）。
    pub fn changed(&mut self, view: ModListView) {
        if self.bound.as_deref() != Some(view.instance.as_str()) {
            return;
        }
        // 这一批结果回来了 ⇒ 那个文件的"在飞"标记可以撤（不管成没成，另一条事件会说）。
        self.pending.clear();
        self.failed = None;
        self.view = Some(view);
    }

    /// 失败落地（只记**当前绑定实例**的；列表那一份保留——用户还能看见旧列表，只是多一条红字）。
    pub fn failed(&mut self, failure: ModFailureView) {
        if self.bound.as_deref() != Some(failure.instance.as_str()) {
            return;
        }
        self.pending.clear();
        self.failed = Some(failure);
    }

    /// 标一个文件"请求在飞"（UI 把那个开关置灰，不做乐观翻面）。
    pub fn mark_pending(&mut self, file: &str) {
        self.pending.insert(file.to_string());
    }

    /// 某个文件的请求是否在飞。
    #[must_use]
    pub fn is_pending(&self, file: &str) -> bool {
        self.pending.contains(file)
    }

    /// 当前绑定实例的列表是否已经有一份快照。
    #[must_use]
    pub fn has_view(&self) -> bool {
        self.view.is_some()
    }
}

impl AppState {
    /// 进/出详情页时对齐绑定（`refresh_view` 调；与 `LaunchForm::sync` 同一处生命周期）。
    pub fn bind_mods(&mut self, instance: Option<&str>) {
        self.mods.bind(instance);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{ModEntryView, ModFailureKind};

    fn view(instance: &str) -> ModListView {
        ModListView {
            instance: instance.to_string(),
            dir: "C:/mc/mods".to_string(),
            exists: true,
            entries: vec![ModEntryView {
                file: "x.jar".to_string(),
                name: "x.jar".to_string(),
                enabled: true,
                bytes: 1,
                old: false,
            }],
            skipped: Vec::new(),
            running: false,
            known: true,
        }
    }

    fn failure(instance: &str) -> ModFailureView {
        ModFailureView {
            instance: instance.to_string(),
            file: "x.jar".to_string(),
            kind: ModFailureKind::Collision,
            detail: "exists".to_string(),
        }
    }

    /// 换实例必须清空（A 的列表不许画在 B 的详情页上）。
    #[test]
    fn binding_another_instance_drops_the_old_view() {
        let mut mods = ModsState::default();
        mods.bind(Some("a"));
        mods.changed(view("a"));
        assert!(mods.has_view());

        mods.bind(Some("b"));
        assert!(!mods.has_view(), "换实例后还留着 A 的列表");
        assert_eq!(mods.bound.as_deref(), Some("b"));

        // 同一个实例再 bind 一次不该清（`refresh_view` 每帧都调）。
        mods.changed(view("b"));
        mods.bind(Some("b"));
        assert!(mods.has_view());
    }

    /// 迟到/串台的结果被丢掉；成功一次清掉旧失败。
    #[test]
    fn late_results_are_dropped() {
        let mut mods = ModsState::default();
        mods.bind(Some("a"));
        mods.changed(view("b"));
        assert!(!mods.has_view(), "B 的列表画到了 A 上");

        mods.failed(failure("a"));
        assert!(mods.failed.is_some());
        mods.changed(view("a"));
        assert!(mods.failed.is_none(), "成功后旧失败没清掉");

        // 不在详情页（`bound = None`）时任何结果都不落地。
        let mut idle = ModsState::default();
        idle.changed(view("a"));
        assert!(!idle.has_view());
    }

    /// 在飞标记：置位后能查，快照回来后自然清空。
    #[test]
    fn pending_tracks_in_flight_toggles() {
        let mut mods = ModsState::default();
        mods.bind(Some("a"));
        mods.changed(view("a"));
        mods.mark_pending("x.jar");
        assert!(mods.is_pending("x.jar"));
        assert!(!mods.is_pending("y.jar"));
        mods.changed(view("a"));
        assert!(!mods.is_pending("x.jar"), "快照回来后还挂着在飞标记");
    }
}
