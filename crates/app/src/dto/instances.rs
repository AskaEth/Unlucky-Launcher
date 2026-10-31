//! 实例相关 DTO：磁贴与列表视图的唯一数据源。

use serde::{Deserialize, Serialize};

use std::path::PathBuf;

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

    /// 稳定 id（`vanilla` / `fabric` / …；GUI 与 CLI 的 `--loader` 同词表）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
            Self::OptiFine => "optifine",
            Self::LiteLoader => "liteloader",
        }
    }

    /// 从稳定 id 解析（未知给 `None`，由调用方决定报错文案）。
    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw.trim().to_ascii_lowercase().as_str() {
            "vanilla" | "none" | "" => Self::Vanilla,
            "fabric" => Self::Fabric,
            "quilt" => Self::Quilt,
            "forge" => Self::Forge,
            "neoforge" => Self::NeoForge,
            "optifine" => Self::OptiFine,
            "liteloader" => Self::LiteLoader,
            _ => return None,
        })
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
    Failed {
        /// 失败原因（用户可见文案）
        reason: String,
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
        };
        let text = serde_json::to_string(&instance).expect("序列化");
        let back: InstanceSummary = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, instance);
    }
}
