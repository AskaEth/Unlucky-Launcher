//! Java 分组消息的落点（M175）：扫描 / 手动路径 / 默认 JDK / 内存 / JVM 参数。
//!
//! 消息口在 `pages/settings/java.rs`（`pages::settings::java::Message`）；状态与读写落在
//! `pages/settings/java/{state,store}.rs`。本文件只有四件事：
//!
//! | 消息 | 做什么 |
//! |---|---|
//! | `Scan` / `CancelScan` | 起一次后台真扫盘 / 置取消位（进度由心跳灌回状态） |
//! | `AddDraft` / `Add` | 只改草稿 / 严格校验（真跑 `java -version`）+ 写手动名单 |
//! | `SelectRuntime` | 写 `java.selected_path`（**立刻落盘**，与「设为默认」的语义一致） |
//! | `EditMemory` / `EditJvm` / `Save` | 改草稿 / 校验后写 `java.default_memory_mb` + `java.extra_args` |
//!
//! ## 为什么「设为默认」不攒到「保存」里
//!
//! 它与内存 / JVM 参数是两件事：前者是**选择**（点哪个用哪个，与方案里其它单选控件一致），
//! 后者是**编辑**（有草稿、有脏标记、有保存）。把选择也攒进保存队列，用户点完「设为默认」看到
//! 界面还标着旧的默认值，只会以为没生效。
//!
//! ## 版本门槛（「为什么选它」那句话的数据）
//!
//! `requirement_for(mc_version)` 是 core 的纯函数（静态表，读不到版本 JSON 时也成立）。
//! 页面自己不看盘：本文件在每次 hydrate 时算好 [`Context`] 塞进状态，渲染器只负责画。

use super::super::*;
use crate::app::diag::diag;

use crate::pages::settings::java::{Context, Layer, Notice, Requirement};

/// M197 的两个落点（各自一个 `impl App` 块）：真下载一个 JDK / 三个设置键。
mod downloads;
mod prefs;

impl App {
    /// Java 分组的一条消息。
    pub(super) fn on_settings_java(
        &mut self,
        message: pages::settings::java::Message,
    ) -> Task<Message> {
        use pages::settings::java::Message;

        match message {
            Message::Scan => return self.start_java_scan(),
            Message::CancelScan => {
                self.settings.java.scan.cancel();
                self.hydrate_java_scan();
            }
            Message::AddDraft(value) => {
                self.settings.java.add_draft = value;
                self.settings.java.notice = None;
                self.view_dirty = true;
            }
            Message::Add => self.add_java_manual(),
            Message::SelectRuntime(binary) => self.select_java_default(&binary),
            Message::EditMemory(value) => {
                self.settings.java.memory = value;
                self.touch_java();
            }
            Message::EditJvm(value) => {
                self.settings.java.jvm = value;
                self.touch_java();
            }
            Message::Save => self.save_java_settings(),
            // M197：真下载一个 JDK + 三个设置键。落点各自一个 `impl App` 块（见 `mod` 声明）。
            Message::Download(major) => return downloads::on_download(self, major),
            Message::EditInstallRoot(value) => return prefs::on_edit_install_root(self, value),
            Message::ToggleAutoInstall(on) => return prefs::on_toggle_auto_install(self, on),
            Message::MoveVendor { index, up } => return prefs::on_move_vendor(self, index, up),
            Message::ResetPriority => return prefs::on_reset_priority(self),
            Message::SavePrefs => return prefs::on_save_prefs(self),
        }

        Task::none()
    }

    /// 三键写盘成功之后，请 presenter 重发一次设置快照（M204）。
    ///
    /// ## 为什么需要这一拍
    ///
    /// 本组的三键（`java.default_memory_mb` / `java.extra_args` / `java.selected_path`）走的是页面
    /// 自己的 `SettingStore`（`pages/settings/java/store.rs`：`set` 几个键 + **一次** `save()`），
    /// **不经过** `AppCommand::SetSetting` 的写盘分支。而 `CoreEvent::SettingsChanged` 全仓只有两处
    /// 发出（`dispatch.rs` 的 `SetSetting` 分支、`host/mod.rs` 的 `refresh_all`）——不请一次，
    /// `AppState::settings_groups` 里 java 组那几行会停在旧值，直到下一次任何别的写入。
    ///
    /// ## 为什么不是 `AppCommand::RefreshAll`
    ///
    /// `refresh_all()` 除重发快照之外还会 `versions::spawn_list`（拉版本清单）与
    /// `start_update_check`（自更新检查）——每点一次「保存内存」打两个网络请求，代价与动作不相称。
    /// 动作键 `JAVA_REFRESH_ACTION` 走的是同一条 `SetSetting` 分支的四个副作用臂，不写盘、不发网络。
    ///
    /// ## 抓得住 / 抓不住
    ///
    /// 抓得住：`tests/java_refresh.rs` 的 `saving_the_java_form_refreshes_the_host_settings_snapshot`
    /// 用**真 `CoreHost` + 临时数据根**跑完整链，摘掉这一行的 dispatch 就红（实测）。
    /// 抓不住：presenter 是否会因为**别的原因**（下一次外部写盘）自己刷新——所以那条用例的判据是
    /// 「写盘后**当场**快照里有新值」，而不是「最终会有」。
    fn request_settings_refresh(&mut self) {
        self.dispatch(AppCommand::SetSetting {
            key: unlucky_presenter::dto::JAVA_REFRESH_ACTION.to_string(),
            value: String::new(),
        });
    }

    /// 记一次「草稿变了」：清掉旧回执（它已经在说另一份配置了），并让视图重建。
    fn touch_java(&mut self) {
        self.settings.java.dirty = true;
        self.settings.java.notice = None;
        self.view_dirty = true;
    }

    /// 从真值（`config/launcher.json` + 手动名单 + 扫描快照）灌一次状态。
    ///
    /// 什么时候调：① 构造之后（首屏就显示已存的值）；② 配置文件被外部改过（`update/clock.rs`
    /// 的 mtime 轮询）。**脏草稿不覆盖**（`State::hydrate` 自己判）。
    ///
    /// 首次调用会**自动发起一次扫描**（用户实测的头一条抱怨就是「设置页内并没有自动检测本机已
    /// 安装过的 Java 路径」）：已经扫完 / 正在扫 / 截图通道都不重扫，所以它只发生一次。
    ///
    /// 截图通道的两档：`ULL_JAVA_STATE=<empty|two|editing>` 用种子（D30 可复现）；
    /// `ULL_JAVA_STATE=live` 走**真路径**（读本机配置 + 真扫盘），用来人工看一眼「别的机器上到底
    /// 扫到了什么」——它不是出图档，所以扫描判据按产品路径算。
    pub(in crate::app) fn hydrate_java_settings(&mut self) {
        let seeded = self.options.screenshot.is_some();
        if seeded {
            let raw = std::env::var("ULL_JAVA_STATE").unwrap_or_else(|_| "two".to_string());
            if raw.trim() != "live" {
                if let Some(state) = pages::settings::java::State::screenshot_seed(&raw) {
                    self.settings.java = state;
                }
                return;
            }
            // `ULL_JAVA_STATE=live`：不灌种子，继续往下走真路径（读本机配置 + 真扫盘）。
        }
        let Some(config) = pages::settings::java::store::config_of(&self.bridge) else {
            self.settings.java = pages::settings::java::State::default();
            return;
        };
        let snapshot = pages::settings::java::store::load(&config);
        self.settings.java.manual = pages::settings::java::store::load_manual(&config);
        let changed = self.settings.java.hydrate(snapshot);
        // M197：三个设置键（安装目录 / 发行版顺序 / 缺 JDK 时自动准备）+ 它们的生效值与来源层。
        let prefs_changed = self
            .settings
            .java
            .prefs
            .hydrate(pages::settings::java::prefs_store::load(&config));
        self.settings.java.effective =
            pages::settings::java::prefs_store::effective(&config, &self.settings.java.prefs);
        let scan_changed = self.hydrate_java_scan();
        self.settings.java.context = self.java_context();
        if changed || prefs_changed || scan_changed {
            self.view_dirty = true;
        }
        // 自动扫一次（首屏就该有真数据）。**没有 `cfg!(test)` 分叉**：单测走的也是这条生产入口，
        // 删掉这一行 / 把实参翻成 `true` ⇒ `tests/java.rs` 的
        // `the_first_hydrate_starts_a_real_scan_by_itself` 立刻红（r3 复审 P3-3 的缺口）。
        //
        // 实参为什么恒为 `false`：截图档的 `empty|two|editing` 在上面就 `return` 了，能走到这里的
        // 只有「不是截图」与「`live` 档」两种，而 `live` 按产品路径算（见本函数文档 D30 的例外）。
        self.auto_scan_java_if_due(false);
    }

    /// 该自动扫就发起一次（判据在 [`pages::settings::java::store::auto_scan_due`]）。
    ///
    /// 抽出来是为了让「取消过就不再自动扫」（r2 复审 P2-2）有个窄的落点；而**首屏真的调了它**
    /// 这件事由生产入口自己守（`tests/java.rs` 的那条首屏用例），不再靠 `cfg!(test)` 把调用关掉。
    pub(in crate::app) fn auto_scan_java_if_due(&mut self, screenshot: bool) -> bool {
        let report = self.settings.java.scan.report();
        if pages::settings::java::store::auto_scan_due(&report, screenshot) {
            let _ = self.start_java_scan();
            return true;
        }
        false
    }

    /// 把后台扫描的快照灌进状态。返回是否变了。
    ///
    /// M197 起这一拍**也**灌一次下载作业的快照：`app/update/clock.rs` 每拍只调这一个入口，
    /// 而复用它比改心跳的调用点便宜（`clock.rs` 不在本 mission 的 scope 里）。
    /// 代价写在 `settings/java/downloads.rs`：下载期间进度按 1 Hz 刷新。
    pub(in crate::app) fn hydrate_java_scan(&mut self) -> bool {
        let report = self.settings.java.scan.report();
        let mut changed = self.settings.java.apply_scan(&report);
        if self.hydrate_java_downloads() {
            changed = true;
        }
        if changed {
            self.view_dirty = true;
        }
        changed
    }

    /// 起一次扫描（真扫盘：托管根 + `JAVA_HOME`/`PATH`/标准安装位置 + 手动名单）。
    fn start_java_scan(&mut self) -> Task<Message> {
        let Some(config) = pages::settings::java::store::config_of(&self.bridge) else {
            self.settings.java.notice = Some(Notice::NoStorage);
            self.view_dirty = true;
            return Task::none();
        };
        let manual = pages::settings::java::store::load_manual(&config)
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>();
        let roots = pages::settings::java::store::managed_roots(&config);
        self.settings.java.notice = None;
        if self.settings.java.scan.start(roots, manual, true) {
            self.hydrate_java_scan();
        }
        self.view_dirty = true;
        Task::none()
    }

    /// 手动加一条 JDK 路径：严格校验 → 进名单 → 立刻出现在列表里。
    fn add_java_manual(&mut self) {
        let Some(config) = pages::settings::java::store::config_of(&self.bridge) else {
            self.settings.java.notice = Some(Notice::NoStorage);
            self.view_dirty = true;
            return;
        };
        let draft = self.settings.java.add_draft.clone();
        match pages::settings::java::store::add_manual(&config, &draft) {
            Ok((binary, row)) => {
                self.settings.java.manual = pages::settings::java::store::load_manual(&config);
                if !self
                    .settings
                    .java
                    .runtimes
                    .iter()
                    .any(|known| pages::settings::java::same_path(&known.binary, &binary))
                {
                    self.settings.java.runtimes.push(row);
                    self.settings
                        .java
                        .runtimes
                        .sort_by_key(|runtime| runtime.major);
                }
                self.settings.java.add_draft = String::new();
                // 手动加的这条已经是「盘上真扫到的一行」，所以顺便把「还没扫描过」这一档收掉：
                // 否则列表里明明有一行，状态行却写着「还没扫描过」。
                self.settings.java.scanned = true;
                self.settings.java.notice = Some(Notice::Added(binary));
            }
            Err(refusal) => self.settings.java.notice = Some(Notice::Refused(refusal)),
        }
        self.view_dirty = true;
    }

    /// 把某个运行时设为默认（写 `java.selected_path`，写完读回）。
    fn select_java_default(&mut self, binary: &str) {
        let Some(config) = pages::settings::java::store::config_of(&self.bridge) else {
            self.settings.java.notice = Some(Notice::NoStorage);
            self.view_dirty = true;
            return;
        };
        match pages::settings::java::store::set_selected(&config, binary) {
            Ok(snapshot) => {
                // **不许清脏**（r1 复审 P2-1）：这一次只写了 `java.selected_path`，内存与 JVM
                // 参数的草稿一个字节都没落盘。`hydrate` 在 `dirty` 时本来就保留草稿、只刷新
                // `selected` / `saved_*`，所以这里唯一正确的做法是什么都不动。
                self.settings.java.hydrate(snapshot);
                self.settings.java.context = self.java_context();
                self.settings.java.notice = Some(Notice::DefaultSet(binary.to_string()));
                // M204：这一笔也写进 launcher.json 了（`java.selected_path`），请 host 重发快照。
                self.request_settings_refresh();
            }
            Err(error) => self.settings.java.notice = Some(write_notice(error)),
        }
        self.view_dirty = true;
    }

    /// 保存内存 + JVM 参数：本地校验 → 落盘 → 用**落盘后读回来的真值**刷新。
    fn save_java_settings(&mut self) {
        let state = &self.settings.java;
        if let Err(refusal) = state.validate() {
            self.settings.java.notice = Some(Notice::Refused(refusal));
            self.view_dirty = true;
            return;
        }
        let Some(config) = pages::settings::java::store::config_of(&self.bridge) else {
            self.settings.java.notice = Some(Notice::NoStorage);
            self.view_dirty = true;
            return;
        };
        match pages::settings::java::store::save(&config, &self.settings.java) {
            Ok(snapshot) => {
                // 先清脏再 hydrate：我们刚写进去的就是这一份，不存在「冲掉用户输入」。
                self.settings.java.dirty = false;
                self.settings.java.hydrate(snapshot);
                self.settings.java.context = self.java_context();
                self.settings.java.notice = Some(Notice::Saved);
                // M204：内存 + JVM 参数已经落盘（一次 `store.save()`），请 host 重发快照。
                self.request_settings_refresh();
            }
            Err(error) => self.settings.java.notice = Some(write_notice(error)),
        }
        self.view_dirty = true;
    }

    /// 当前实例的上下文（版本门槛 + 生效内存来自哪一层）。
    ///
    /// 「当前实例」= 路由栈顶是详情页时那个实例，否则列表第一行。没有实例 ⇒ 空上下文
    /// （页面就不画门槛那一行，也不假装有要求）。
    fn java_context(&self) -> Context {
        let tiles = self.state.tiles();
        let routed = match self.state.route() {
            unlucky_presenter::router::Route::Detail(id) => {
                tiles.iter().find(|cell| &cell.instance.id == id)
            }
            _ => None,
        };
        let Some(cell) = routed.or_else(|| tiles.first()) else {
            return Context::default();
        };
        let instance = cell.instance;
        let requirement = unlucky_core::game::java::requirement_for(&instance.mc_version);
        Context {
            required: Some(Requirement {
                instance: instance.name.clone(),
                version: instance.mc_version.clone(),
                minimum: requirement.minimum,
                maximum: requirement.maximum,
            }),
            layer: Some(match instance.launch.memory_mb {
                Some(mb) => Layer::Instance {
                    instance: instance.name.clone(),
                    mb,
                },
                None => Layer::Global,
            }),
            effective_memory_mb: Some(instance.launch.memory_effective_mb),
        }
    }
}

/// 写盘失败 → 回执：三档都走 i18n key。
///
/// `WriteError::Schema` 里装的是 `core::setting::StoreError` 的 `Display`（**中文**），
/// 直出会把 `Not saved: IO 失败 @ …` 塞进英文界面（r2 复审 P3-3）——所以只进 stderr 日志，
/// 界面上给一句页面自己的「写不进去」。
///
/// `pub(super)`：`tests/java.rs` 直接钉这一层的映射（M199 起页面侧已经撞不到 schema 那一档）。
pub(super) fn write_notice(error: pages::settings::java::store::WriteError) -> Notice {
    use pages::settings::java::store::WriteError;
    match error {
        WriteError::Refused(refusal) => Notice::Refused(refusal),
        WriteError::Unwritable(reason) => Notice::Unwritable(reason),
        WriteError::Schema(detail) => {
            diag!("[java] 设置写盘失败：{detail}");
            Notice::WriteFailed
        }
    }
}
