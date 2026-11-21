//! 「设置 → AI」快照（`Plan`）：provider 配置 + 数据根 + 脱敏器（**YOLO 不在这里**，见下）。
//!
//! 从 `real.rs` 拆出来的唯一理由是**400 行红线**（`guards.yml` 的 file-size 门禁）：
//! 那边一度 397 行、只剩 3 行余量，而装配/映射/闸门三件事已经各在自己的文件里了。
//! 拆分是纯搬移，行为一字未改。
//!
//! ## YOLO 不是这里面的一份快照（M389）
//!
//! `Plan` 里的 provider 四项与数据根确实是装配期的一份（换它们要重建会话），但 **YOLO 不是**：
//! [`Plan::yolo_switch`] 给的是「每次判定现读 `launcher.json` 的 `ai.yolo`」的探针
//! （[`unlucky_agent::confirm::YoloSwitch::live`]）。用户在设置页关掉 YOLO，**同一条会话**的
//! 下一次高敏感写就会重新问人——这正是 `gapreg` X6 要修的那条。

use std::path::{Path, PathBuf};

use unlucky_agent::brain::provider::ProviderConfig;
use unlucky_agent::brain::session::SessionConfig;
use unlucky_agent::confirm::{ConfirmPolicy, YoloSwitch};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use unlucky_cli::commands::agent::probe::{self, AiSettings, Missing};
use unlucky_cli::context::Ctx;

use super::mapper::Scrubber;

/// 装配真后端需要的一份快照（设置路径 → provider 配置 + 数据根；**YOLO 是探针，不是快照**）。
#[derive(Clone)]
pub(crate) struct Plan {
    /// provider 四项（`Debug` 里 key 是掩码）。
    pub(crate) config: ProviderConfig,
    settings: AiSettings,
    /// 数据根（审计落 `<root>/agent/audit/`）。
    pub(crate) data_root: PathBuf,
    /// 这次读的是哪份 `launcher.json`（日志/取证对照用；**YOLO 探针也读它**）。
    pub(crate) config_path: PathBuf,
    /// 空闲时限（M388）：`ai.` 的 schema 里还没有这一项（core 不在 M388 的 scope），所以这里是
    /// **默认值 + 环境变量覆盖**；环境变量由内核每轮现读，这里的值只是「配置层的那一份」。
    pub(crate) request_timeout: std::time::Duration,
}

impl Plan {
    pub(crate) fn resolve() -> Result<Self, Missing> {
        let path = launcher_config_path()?;
        Self::at(&path)
    }

    /// 从指定设置文件装配（测试入口：把 `launcher.json` 钉在临时目录里）。
    pub(crate) fn at(config_path: &Path) -> Result<Self, Missing> {
        let settings = AiSettings::load(config_path);
        let config = settings.to_config()?;
        // `<home>/config/launcher.json` → `<home>`（与 `platform::data::layout` 同一条推导）
        let data_root = config_path
            .parent()
            .and_then(Path::parent)
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        Ok(Self {
            config,
            settings,
            data_root,
            config_path: config_path.to_path_buf(),
            request_timeout: unlucky_agent::brain::DEFAULT_IDLE_TIMEOUT,
        })
    }

    /// 换空闲时限（**用例入口**：让「超时」这件事在测试里只花几百毫秒）。
    ///
    /// 生产路径没有调用点：那边走 `SessionConfig::new` 的默认值 + `ULL_AGENT_TIMEOUT_SECS`。
    #[cfg(test)]
    pub(crate) fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    pub(crate) fn scrubber(&self) -> Scrubber {
        let secret = self.settings.api_key.clone();
        Scrubber::new(move |text: &str| probe::scrub(text, secret.as_deref()))
    }

    /// YOLO 的**活开关**（M389）：每次判定现读这一份 `launcher.json`。
    ///
    /// 谁写这份文件都能立刻生效：设置页的 `ai.yolo` 行（`host::settings::set` → 原子写）、
    /// `ull config set ai.yolo …`、外部编辑器、以及 AI 自己的 `set_config("ai.yolo")`。
    /// 读不到/坏文件 ⇒ [`yolo_of`] 返回 `false`（**fail-closed**：会问人，不拿旧的 `true` 兜底）。
    pub(crate) fn yolo_switch(&self) -> YoloSwitch {
        let path = self.config_path.clone();
        YoloSwitch::live(move || yolo_of(&path))
    }

    /// 此刻 YOLO 是开还是关（日志与用例读这一个；**不是**装配期抄下来的值）。
    pub(crate) fn yolo_now(&self) -> bool {
        self.yolo_switch().get()
    }

    pub(crate) fn session_config(&self) -> SessionConfig {
        SessionConfig::new(
            self.config.clone(),
            // 装配期这一份只是「没有探针时的兜底形状」；下面的 `with_yolo_switch` 才是判据。
            ConfirmPolicy {
                yolo: self.yolo_now(),
            },
            self.data_root.clone(),
        )
        .with_yolo_switch(self.yolo_switch())
        .with_request_timeout(self.request_timeout)
    }
}

/// 设置页写的那一份 `launcher.json`（`SettingsBridge` 与 `Ctx` 走同一条 `Resolver` 配方）。
fn launcher_config_path() -> Result<PathBuf, Missing> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let ctx = Ctx::resolve_in(&unlucky_cli::cli::Globals::default(), exe_dir).map_err(|error| {
        // 数据根解析不出来 ≠「没填端点」：日志里要说真话（复审 r1 的 #9），
        // 但返回类型只有「缺哪一项」这几种，仍按最接近的 `base_url` 收场。
        eprintln!("[agent] 数据根解析失败（{error}），AI 配置无从读取");
        Missing::BaseUrl
    })?;
    let from_cli = ctx.launcher_config();
    // 数据根解析有两条入口（presenter 的 `SettingsBridge` 与 cli 的 `Ctx`）。它们今天同源，
    // 但**不同源时工具改的就不是用户在设置页看到的那份文件**——那件事必须吵，不能安静地写偏。
    let bridge = unlucky_presenter::settings_bridge::SettingsBridge::open();
    if let Some(other) = bridge.path().filter(|other| *other != from_cli.as_path()) {
        eprintln!(
            "[agent] 设置文件两条解析路径不一致：cli={} bridge={}",
            from_cli.display(),
            other.display()
        );
    }
    Ok(from_cli)
}

/// 读 `ai.yolo`（设置 → AI 的 YOLO 开关）。**读不到/坏文件按关处理 = fail-closed**：
/// 安全开关的默认必须偏向保守（宁可多问一次，也不许拿一个旧的 `true` 兜底）。
fn yolo_of(config_path: &Path) -> bool {
    let store = SettingStore::load(config_path, launcher_schema());
    matches!(store.doc().effective_bool("ai.yolo"), Some(true))
}
