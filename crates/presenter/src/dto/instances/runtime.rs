//! 进度 / 阶段 / 运行时状态 / 实例摘要（磁贴与列表的唯一数据源）。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::super::LaunchArgsView;
use super::{InstanceId, LoaderInfo};

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
    /// 现在给的是结构化原因（见 [`crate::dto::LaunchFailureCause`]）：**没起来**与**起来又崩**是两种形状，
    /// 界面各说各的话（复审 P1-1：早先崩溃被说成「启动游戏进程失败」）。
    Failed {
        /// 这一场为什么是失败态
        cause: crate::dto::LaunchFailureCause,
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
