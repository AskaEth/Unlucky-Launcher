//! 更新状态的**会话层**：一份 `UpdateView`、主配置里的两个开关、以及把 `CoreHost` 接上后台线程。
//!
//! 与 [`super::update`] 的分工：那边是「一次检查/应用的纯做法」（选源、调 core、映射 DTO），
//! 这边是「谁来记着它、谁来派发」。拆开是因为两边都各有一批判据要写清（静默检查的开关、
//! 拒绝后不再打扰、应用失败不许丢掉候选），塞在一个文件里会越过 400 行硬线。

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;
use unlucky_core::setting::update_prefs::{self, Migration};
use unlucky_core::update::{Channel, Version};

use crate::dto::{CoreEvent, ReadyUpdate, UpdateOutcome, UpdateView};

use super::env::HostEnv;
use super::update::{current_version, outcome_of, ApplyReport, Factory, Paths, Report};
use super::CoreHost;

/// 会话级更新状态（提示条 / 设置页共用一份）。
#[derive(Debug)]
pub struct Runtime {
    view: UpdateView,
    /// `update.auto_check` 的生效值（缓存：`should_check_on_start` 在 `refresh_all` 里被调）。
    auto_check: bool,
    factory: Factory,
    /// `config/launcher.json`（两个开关的真相源；写它就走 `core::setting`）。
    launcher: PathBuf,
    /// 建会话时那次遗留文件迁移的结局（`Nothing` = 没有旧文件）。
    migration: Migration,
    /// 有一条检查/应用正在后台跑（防重复派发；进程内独占）。
    inflight: bool,
}

impl Runtime {
    /// 按数据根装配（顺带把 M159 的 `cache/update/prefs.json` 迁进主配置）。
    #[must_use]
    pub fn for_env(env: &HostEnv) -> Self {
        Self::with_factory(env, Factory::for_env(env))
    }

    /// 用给定装配（单测：临时目录 + 本地目录源）。
    #[must_use]
    pub fn with_factory(env: &HostEnv, factory: Factory) -> Self {
        let paths = Paths::for_env(env);
        // 迁移在建会话时跑一次：**幂等**（源指纹闸门），没有旧文件时是纯 stat 一次。
        let migration = update_prefs::migrate(paths.targets(), now_ms());
        let store = SettingStore::load(&paths.launcher, launcher_schema());
        let auto_check = update_prefs::auto_check(store.doc());
        let channel = update_prefs::channel(store.doc()).to_string();
        Self {
            view: UpdateView::new(current_version(), &channel, auto_check),
            auto_check,
            factory,
            launcher: paths.launcher,
            migration,
            inflight: false,
        }
    }

    /// 当前快照。
    #[must_use]
    pub fn view(&self) -> &UpdateView {
        &self.view
    }

    /// 重建「怎么造 Job」的装配（M165 复审 P2-1）。
    ///
    /// 自更新的 `Factory` 常驻持有下载引擎，`download::shared::forget` 换不掉它；「重新检测地区」
    /// 要生效就得把新引擎接过来。只换 `factory`，**不动** `view` / `inflight` / 迁移收据：
    /// 后台在飞的那条 Job 在派发时（`begin_check` / `begin_apply` → `Job`）就已经把 factory
    /// 克隆进线程了，所以重建**不会**打断正在跑的检查/应用，只影响下一次派发。
    pub fn rebuild_factory(&mut self, env: &HostEnv) {
        self.factory = Factory::for_env(env);
    }

    /// 装配里那个下载引擎（单测断言「换了引擎」用；`Dir` 形态没有引擎）。
    #[cfg(test)]
    pub(super) fn factory_engine(
        &self,
    ) -> Option<&std::sync::Arc<unlucky_core::download::DownloadEngine>> {
        match &self.factory {
            Factory::Default { engine, .. } => Some(engine),
            Factory::Dir { .. } => None,
        }
    }

    /// 当前快照（克隆给事件/线程）。
    #[must_use]
    pub fn snapshot(&self) -> UpdateView {
        self.view.clone()
    }

    /// 「启动时静默检查」的生效值（来自 `launcher.json` 的 `update.auto_check`）。
    #[must_use]
    pub const fn auto_check(&self) -> bool {
        self.auto_check
    }

    /// 建会话时那次迁移的结局（`Nothing` = 没有遗留文件；收据路径见 [`Migration::receipt`]）。
    #[must_use]
    pub const fn migration(&self) -> &Migration {
        &self.migration
    }

    /// 后台有没有一条在跑。
    #[must_use]
    pub const fn inflight(&self) -> bool {
        self.inflight
    }

    /// 本进程版本解析（失败时 `None`：版本号是编译进来的常量，理论上不可能失败）。
    #[must_use]
    pub fn current(&self) -> Option<Version> {
        Version::parse(current_version()).ok()
    }

    /// 启动时要不要静默检查（开关关掉 ⇒ 启动**一次网络都不发**）。
    #[must_use]
    pub fn should_check_on_start(&self) -> bool {
        self.auto_check && !self.inflight
    }

    /// 检查/应用用的渠道：**每次都从主配置读**（用户在 CLI 里改了 `update.channel` 之后，
    /// 下一次检查就按新渠道走，不用重启）。
    ///
    /// 认不出的值（手改文件 / 未来的渠道）回出厂渠道：schema 已经把写入侧卡在允许值上，
    /// 读侧只兜底、不报错。
    #[must_use]
    pub fn channel(&self) -> Channel {
        let store = SettingStore::load(&self.launcher, launcher_schema());
        Channel::parse(update_prefs::channel(store.doc())).unwrap_or(Channel::Stable)
    }

    /// 开关「启动时静默检查」并写进主配置（`update.auto_check`）。
    ///
    /// # Errors
    ///
    /// 落盘失败返回人读文案（本会话的取值仍然生效——不因为写不进去就把开关弹回去）。
    pub fn set_auto_check(&mut self, on: bool) -> Result<(), String> {
        self.auto_check = on;
        self.view.auto_check = on;
        let mut store = SettingStore::load(&self.launcher, launcher_schema());
        store
            .set(update_prefs::KEY_AUTO_CHECK, json!(on))
            .map_err(|error| format!("写 update.auto_check 被拒：{error}"))?;
        match store.save() {
            Ok(saved) if saved.wrote() || !store.is_dirty() => Ok(()),
            Ok(_) => Err("update.auto_check 没有落盘（配置只读或文件不可信）".to_string()),
            Err(error) => Err(format!("写 {} 失败：{error}", self.launcher.display())),
        }
    }

    /// 开始一次检查：置「正在查」并返回要交给后台线程的 [`unlucky_core::update::Job`]
    /// （`None` = 已经有一条在跑）。
    ///
    /// `manual` = 用户点的「立即检查」：会清掉「忽略这个版本」，并让失败也弹提示条。
    pub fn begin_check(&mut self, manual: bool) -> Option<unlucky_core::update::Job> {
        if self.inflight {
            return None;
        }
        self.inflight = true;
        self.view.outcome = UpdateOutcome::Checking;
        self.view.silent = !manual;
        self.view.applying = false;
        self.view.channel = self.channel().as_str().to_string();
        if manual {
            // 手动检查 = 用户要看真结果，把「忽略这个版本」清掉。
            self.view.skipped = None;
        }
        Some(self.factory.job())
    }

    /// 开始应用：`None` = 没有在跑的必要（正在忙 / 没有候选 / 已经就位）。
    pub fn begin_apply(&mut self) -> Option<unlucky_core::update::Job> {
        if self.inflight || self.view.ready.is_some() || self.view.outcome.candidate().is_none() {
            return None;
        }
        self.inflight = true;
        self.view.applying = true;
        Some(self.factory.job())
    }

    /// 后台回报进快照（**唯一**的写入口：检查与应用都走这里）。
    pub fn adopt(&mut self, report: Report) {
        self.inflight = false;
        match report {
            Report::Checked(outcome) => {
                // 「上次检查」只该被**检查**刷新：应用回来的那一刻不是一次检查。
                self.view.checked_at = super::update::stamp();
                // 「忽略这个版本」记的是版本号，所以**这里不需要动它**：
                // 同一版本回来仍被压住，更高版本自然就提示了（见 `UpdateView::skipped`）。
                // 手动检查那一支的清空在 `begin_check(manual)` 里。
                self.view.outcome = outcome;
            }
            Report::Applied(ApplyReport::Installed {
                version,
                path,
                backup,
            }) => {
                self.view.applying = false;
                self.view.outcome = UpdateOutcome::UpToDate {
                    latest: Some(version.clone()),
                };
                self.view.ready = Some(ReadyUpdate {
                    version,
                    path,
                    backup,
                });
            }
            Report::Applied(ApplyReport::Nothing(outcome)) => {
                self.view.applying = false;
                self.view.outcome = outcome;
            }
            Report::Applied(ApplyReport::Failed(failure)) => {
                // 应用失败要说清是哪一步（校验不符 / 替换被占用）。候选随 `outcome` 一起换成
                // `Failed`，**重试走「立即检查」重新复查**（清单只有几 KB，apply 本来也要复查一次）：
                // 这样重试拿到的永远是最新那一份，而不是几分钟前的旧候选。失败也不再算「静默」，
                // 提示条要把原因摆出来。
                self.view.applying = false;
                self.view.outcome = UpdateOutcome::Failed(failure);
                self.view.silent = false;
            }
        }
    }

    /// 用户忽略**当前这个候选版本**：记下版本号（候选仍留在快照里，设置页可手动装）。
    ///
    /// 没有候选时是空操作（`skipped` 保持原样）：没版本可忽略就不该写进一个「你别问」的状态。
    pub fn dismiss(&mut self) {
        if let Some(candidate) = self.view.outcome.candidate() {
            self.view.skipped = Some(candidate.version.clone());
        }
    }
}

impl CoreHost {
    /// 发布更新快照（唯一的出口；`CoreEvent::Update` 是整份替换）。
    pub(super) fn publish_update(&mut self) {
        let view = self.update.snapshot();
        self.send(CoreEvent::Update(Box::new(view)));
    }

    /// 让自更新子系统手里的引擎跟上共享登记处（M165 复审 P2-1）。
    ///
    /// 自更新的 `Factory` 从 `CoreHost::build` 起就把引擎存进字段，`shared::forget` 只让登记处
    /// 不再持有那一份，换不掉它。所以「重新检测地区」除了失效登记处，还必须调本方法，否则自更新
    /// 会继续按**旧地区档案**展开候选，直到进程结束。
    pub(super) fn refresh_update_factory(&mut self) {
        self.update.rebuild_factory(&self.env);
    }

    /// 派一次检查（`manual` = 设置页点的「立即检查」；`false` = 启动时的静默检查）。
    pub(super) fn start_update_check(&mut self, manual: bool) {
        let Some(current) = self.update.current() else {
            return;
        };
        let Some(job) = self.update.begin_check(manual) else {
            return;
        };
        let channel = self.update.channel();
        // 先把「正在查」发出去：设置页的按钮据此转圈，而不是看起来没反应。
        self.publish_update();
        spawn_check(job, current, channel, self.tx.clone());
    }

    /// 应用更新（下载 → 长度/SHA256 双检 → 替换；全部在后台线程）。
    pub(super) fn start_update_apply(&mut self) {
        let Some(current) = self.update.current() else {
            return;
        };
        let Some(job) = self.update.begin_apply() else {
            return;
        };
        let channel = self.update.channel();
        self.publish_update();
        spawn_apply(job, current, channel, self.tx.clone());
    }
}

/// 起一条检查线程（结果经 [`super::HostTask::Update`] 回来）。
pub fn spawn_check(
    job: unlucky_core::update::Job,
    current: Version,
    channel: Channel,
    tx: super::JobSender,
) {
    super::job::spawn("update-check", move || {
        let report = Report::Checked(outcome_of(job.check(&current, channel)));
        let _ = tx.send(super::HostTask::Update(Box::new(report)));
    });
}

/// 起一条应用线程（下载 + 双检 + 替换都在里面）。
pub fn spawn_apply(
    job: unlucky_core::update::Job,
    current: Version,
    channel: Channel,
    tx: super::JobSender,
) {
    super::job::spawn("update-apply", move || {
        let report = Report::applied(job.apply(&current, channel));
        let _ = tx.send(super::HostTask::Update(Box::new(report)));
    });
}

/// 当前时刻（毫秒 since UNIX_EPOCH；收据用）。
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
