//! 实例相关 DTO：磁贴与列表视图的唯一数据源。

use serde::{Deserialize, Serialize};

pub use super::launch_failure::{LaunchFailureCause, LaunchFailureStage, LaunchFailureView};

use std::path::PathBuf;

use unlucky_core::game::LoaderKind as CoreLoaderKind;

use super::LaunchArgsView;

/// 实例标识：core::game 生成的稳定 id，app 视为不透明字符串。
pub type InstanceId = String;

/// 加载器种类（磁贴徽标 + 详情页版本链）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoaderKind {
    /// 原版（无加载器）
    Vanilla,
    /// Fabric
    Fabric,
    /// Quilt
    Quilt,
    /// Forge
    Forge,
    /// NeoForge
    NeoForge,
    /// OptiFine
    OptiFine,
    /// LiteLoader
    LiteLoader,
}

impl LoaderKind {
    /// 徽标显示文字。
    pub fn label(self) -> &'static str {
        match self {
            Self::Vanilla => "原版",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::OptiFine => "OptiFine",
            Self::LiteLoader => "LiteLoader",
        }
    }

    /// 本 DTO 种类 → core 的宽种类。
    ///
    /// **结构映射，一个 id 字面量都没有**：词表只有一个真相源
    /// （[`CoreLoaderKind::as_str`]），本 crate 写不进第二份。加一支 DTO 种类这里编不过。
    const fn to_core(self) -> CoreLoaderKind {
        match self {
            Self::Vanilla => CoreLoaderKind::Vanilla,
            Self::Fabric => CoreLoaderKind::Fabric,
            Self::Quilt => CoreLoaderKind::Quilt,
            Self::Forge => CoreLoaderKind::Forge,
            Self::NeoForge => CoreLoaderKind::NeoForge,
            Self::OptiFine => CoreLoaderKind::OptiFine,
            Self::LiteLoader => CoreLoaderKind::LiteLoader,
        }
    }

    /// core 的宽种类 → 本 DTO 种类（[`Self::to_core`] 的反向，同样不含字面量）。
    ///
    /// core 给 `game::LoaderKind` 加一支、而 DTO 还没跟上时，这里编不过——这正是要的：
    /// UI 不许「新加载器默默显示成原版」。
    const fn from_core(kind: CoreLoaderKind) -> Self {
        match kind {
            CoreLoaderKind::Vanilla => Self::Vanilla,
            CoreLoaderKind::Fabric => Self::Fabric,
            CoreLoaderKind::Quilt => Self::Quilt,
            CoreLoaderKind::Forge => Self::Forge,
            CoreLoaderKind::NeoForge => Self::NeoForge,
            CoreLoaderKind::OptiFine => Self::OptiFine,
            CoreLoaderKind::LiteLoader => Self::LiteLoader,
        }
    }

    /// 稳定 id（`vanilla` / `fabric` / …；GUI 与 `--json` 同词表）。
    ///
    /// **委派**给 [`CoreLoaderKind::as_str`]（M126）：词表的源头表在 core（0.2.0 已发布契约；
    /// 另一处逐字副本是 `ull help instances` 的词表行），这里只剩一次结构映射。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.to_core().as_str()
    }

    /// 从稳定 id 解析（未知给 `None`，由调用方决定报错文案）。
    ///
    /// 七个 id 的匹配委派给 [`CoreLoaderKind::parse`]；本方法只额外收 `none` / 空串这两个
    /// **GUI 输入别名**（表单里「不装加载器」的写法）——它们不是对外输出的 id，也绝不参与
    /// [`Self::as_str`]。
    pub fn parse(raw: &str) -> Option<Self> {
        let wanted = raw.trim();
        if wanted.is_empty() || wanted.eq_ignore_ascii_case("none") {
            return Some(Self::Vanilla);
        }
        Some(Self::from_core(CoreLoaderKind::parse(wanted)?))
    }

    /// 向导里的可选加载器（顺序即展示顺序）。
    pub fn wizard_choices() -> [Self; 5] {
        [
            Self::Vanilla,
            Self::Fabric,
            Self::Quilt,
            Self::Forge,
            Self::NeoForge,
        ]
    }
}

/// 加载器 + 版本（例如 Fabric 0.16.5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderInfo {
    /// 加载器种类
    pub kind: LoaderKind,
    /// 加载器版本号；原版为空串
    pub version: String,
}

impl Default for LoaderInfo {
    fn default() -> Self {
        Self {
            kind: LoaderKind::Vanilla,
            version: String::new(),
        }
    }
}

impl LoaderInfo {
    /// 徽标文字：原版只写「原版」，装加载器时写「Fabric 0.16.5」。
    pub fn badge(&self) -> String {
        if self.kind == LoaderKind::Vanilla || self.version.is_empty() {
            self.kind.label().to_string()
        } else {
            format!("{} {}", self.kind.label(), self.version)
        }
    }
}

/// 字节进度；`total == 0` 表示总量未知（UI 转不确定态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Progress {
    /// 已完成量
    pub done: u64,
    /// 总量；0 = 未知
    pub total: u64,
}

impl Progress {
    /// 0.0~1.0；未知总量返回 0.0。
    pub fn ratio(self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f32 / self.total as f32).clamp(0.0, 1.0)
        }
    }

    /// 总量是否已知。
    pub fn is_known(self) -> bool {
        self.total > 0
    }
}

/// 启动管线阶段（磁贴进度环的文案来源；D8 的 10 阶段在 P1 细化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchStage {
    /// 下载资源
    Download,
    /// 校验文件
    Verify,
    /// 组装命令行并拉起进程
    Launch,
    /// 进程已起、等待游戏窗口（M28 的窗口检测阶段）
    Window,
}

impl LaunchStage {
    /// 磁贴上的阶段文案。
    pub fn label(self) -> &'static str {
        match self {
            Self::Download => "下载资源",
            Self::Verify => "校验文件",
            Self::Launch => "组装命令行",
            Self::Window => "等待窗口",
        }
    }
}

/// 实例运行时状态（磁贴的四种外观）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InstanceRuntime {
    /// 空闲
    #[default]
    Idle,
    /// 启动管线进行中
    Busy {
        /// 当前阶段
        stage: LaunchStage,
        /// 该阶段进度
        progress: Progress,
    },
    /// 游戏进程存活
    Running {
        /// 进程号（停止/诊断用）
        pid: u32,
    },
    /// 上次启动失败（磁贴显示「重试」）
    ///
    /// M161 之前这里只有一个 `reason: String`，而且里面已经写着「启动失败：…」——
    /// 界面再包一层就成了「英文前缀 + 中文句子」，中文界面则是「启动失败 · 启动失败：…」。
    /// 现在给的是结构化原因（见 [`LaunchFailureCause`]）：**没起来**与**起来又崩**是两种形状，
    /// 界面各说各的话（复审 P1-1：早先崩溃被说成「启动游戏进程失败」）。
    Failed {
        /// 这一场为什么是失败态
        cause: LaunchFailureCause,
    },
}

/// 实例摘要：磁贴/列表视图唯一数据源。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceSummary {
    /// 稳定 id
    pub id: InstanceId,
    /// 显示名
    pub name: String,
    /// 游戏版本（如 1.20.4）
    pub mc_version: String,
    /// 加载器信息
    pub loader: LoaderInfo,
    /// ISO-8601 UTC 最后游玩时间；None = 从未启动
    pub last_played: Option<String>,
    /// 运行时状态
    pub runtime: InstanceRuntime,
    /// 实例图标路径（P0 直接给路径；P1 换成 core 提供的图像句柄）
    pub icon: Option<PathBuf>,
    /// 隔离档位（`isolated` / `shared` / `custom`；未知为空）
    pub isolation: String,
    /// 该实例绑定的版本 id（详情页/启动用）
    pub version_id: String,
    /// 版本本体是否在盘上（缺载荷时磁贴亮黄条）
    pub version_present: bool,
    /// 启动参数（M157：实例配置页「启动参数」区的数据；含三级回落后的生效值与版本门槛）
    pub launch: LaunchArgsView,
}

/// 新建实例向导提交给 host 的请求（已通过向导校验）。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_badges_and_ids() {
        let vanilla = LoaderInfo::default();
        assert_eq!(vanilla.badge(), "原版");
        let fabric = LoaderInfo {
            kind: LoaderKind::Fabric,
            version: "0.16.5".to_string(),
        };
        assert_eq!(fabric.badge(), "Fabric 0.16.5");
        for kind in LoaderKind::wizard_choices() {
            assert_eq!(LoaderKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(LoaderKind::parse("fabric"), Some(LoaderKind::Fabric));
        assert_eq!(LoaderKind::parse("unknown"), None);
    }

    /// 对外契约：七个 id 在测试里**独立抄一遍**（0.2.0 已发布形状），并与 core 的 `ALL` 做
    /// **集合比较**——这是对外词表的守卫（与 CLI 的 help 守卫互补）。显示路径另有守卫：
    /// `presenter::host::game::every_core_loader_kind_reaches_the_tile_as_itself`（按 `ALL` 遍历）。
    ///
    /// 为什么不是编译器管：编译期只拦得到「漏 match」，而新种类在还有 match 的消费方用最省事的
    /// `=> Vanilla` 糊过去**编得过**，那时新加载器会静默显示成原版（`game::loader` 模块文档
    /// 第 ③ 步）。对外词表由下面第 ② 段的集合断言守，显示路径由上面那条测试守。
    ///
    /// 抓得住：改词表（`neoforge` → `neo_forge`）；**新增一家加载器而没主动更新这份契约**
    /// （core 的 `ALL` 多一支 ⇒ 集合不等）；DTO 委派接错支（`OptiFine` 映到 `LiteLoader`）；
    /// `parse` 收进不该有的别名。
    /// 抓不住：core 的解析规则（那是 `game::loader` 的
    /// `every_stable_id_parses_back_to_its_kind` 等测试的职责），以及真二进制输出面的 JSON
    /// （CLI 的 `json_rows_expose_loader_kind_and_version`）。
    #[test]
    fn loader_kind_ids_are_the_released_contract() {
        const CONTRACT: [(LoaderKind, &str); 7] = [
            (LoaderKind::Vanilla, "vanilla"),
            (LoaderKind::Fabric, "fabric"),
            (LoaderKind::Quilt, "quilt"),
            (LoaderKind::Forge, "forge"),
            (LoaderKind::NeoForge, "neoforge"),
            (LoaderKind::OptiFine, "optifine"),
            (LoaderKind::LiteLoader, "liteloader"),
        ];

        // ① DTO 这一层：每支的 id、反查、大小写。
        for (kind, id) in CONTRACT {
            assert_eq!(kind.as_str(), id, "{kind:?} 的稳定 id 变了");
            assert_eq!(LoaderKind::parse(id), Some(kind), "`{id}` 认不回来");
            let upper = id.to_ascii_uppercase();
            assert_eq!(LoaderKind::parse(&upper), Some(kind), "大写 `{upper}`");
        }

        // ② core 的词表 ↔ 这份契约：**集合相等**才过（多一支、少一支、改一个字都红）。
        let mut actual: Vec<&str> = CoreLoaderKind::ALL
            .iter()
            .map(|kind| kind.as_str())
            .collect();
        actual.sort_unstable();
        let mut expected: Vec<&str> = CONTRACT.iter().map(|(_, id)| *id).collect();
        expected.sort_unstable();
        assert_eq!(
            actual, expected,
            "core 的 `LoaderKind::ALL` 必须逐字等于对外契约：加一家加载器时先改这里 + help"
        );

        // GUI 输入别名（不是对外 id）：不装加载器有三种写法；别名与拼法都不许混进词表。
        for alias in ["none", "", "   "] {
            assert_eq!(
                LoaderKind::parse(alias),
                Some(LoaderKind::Vanilla),
                "`{alias}`"
            );
        }
        assert_eq!(LoaderKind::parse("neo-forge"), None, "别名只归安装侧");
    }

    #[test]
    fn progress_ratio_handles_unknown_total() {
        assert_eq!(Progress { done: 1, total: 0 }.ratio(), 0.0);
        assert!((Progress { done: 1, total: 4 }.ratio() - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn create_request_defaults_name_to_version() {
        let request = CreateRequest {
            version: "1.20.4".to_string(),
            loader: LoaderKind::Vanilla,
            loader_version: String::new(),
            name: "  ".to_string(),
        };
        assert_eq!(request.instance_name(), "1.20.4");
    }

    #[test]
    fn instance_summary_is_serde_round_trip_safe() {
        let instance = InstanceSummary {
            id: "inst-1".to_string(),
            name: "生存".to_string(),
            mc_version: "1.20.4".to_string(),
            loader: LoaderInfo {
                kind: LoaderKind::Fabric,
                version: "0.16.5".to_string(),
            },
            last_played: Some("2026-10-31T12:00:00.000Z".to_string()),
            runtime: InstanceRuntime::Busy {
                stage: LaunchStage::Window,
                progress: Progress { done: 1, total: 2 },
            },
            icon: None,
            isolation: "isolated".to_string(),
            version_id: "1.20.4".to_string(),
            version_present: true,
            launch: LaunchArgsView::default(),
        };
        let text = serde_json::to_string(&instance).expect("序列化");
        let back: InstanceSummary = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, instance);
    }
}
