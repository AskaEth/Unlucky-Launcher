//! 实例相关 DTO：磁贴与列表视图的唯一数据源。

use std::path::PathBuf;

/// 实例标识：core::game 生成的稳定 id，app 视为不透明字符串。
pub type InstanceId = String;

/// 加载器种类（磁贴徽标 + 详情页版本链）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

/// 加载器 + 版本（例如 Fabric 0.16.5）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderInfo {
    /// 加载器种类
    pub kind: LoaderKind,
    /// 加载器版本号；原版为空串
    pub version: String,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchStage {
    /// 下载资源
    Download,
    /// 校验文件
    Verify,
    /// 组装命令行并拉起进程
    Launch,
}

impl LaunchStage {
    /// 磁贴上的阶段文案。
    pub fn label(self) -> &'static str {
        match self {
            Self::Download => "下载资源",
            Self::Verify => "校验文件",
            Self::Launch => "启动中",
        }
    }
}

/// 实例运行时状态（磁贴的四种外观）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, PartialEq)]
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
}
