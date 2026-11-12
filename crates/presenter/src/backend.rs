//! 后端选择与启动选项：真实 core host 优先，解析不出数据根时退回 mock。

use std::path::{Path, PathBuf};

use crate::dto::Backend;
use crate::host::CoreHost;
use crate::host::HostEnv;
use crate::mock::MockBackend;

/// 启动选项（截图/演示通道用；正常启动全用默认值）。
#[derive(Debug, Clone, Default)]
pub struct AppOptions {
    /// 钉死数据根（`<root>/.ULL` + `<root>/.minecraft`）；None = 按便携配置解析
    pub data_root: Option<PathBuf>,
    /// 强制 mock 后端（UI 回归/无数据根演示）
    pub mock: bool,
}

/// `<root>/.ULL/config/launcher.json`（与 `SettingsBridge::open` 同落点）。
pub fn config_path(root: &Path) -> PathBuf {
    root.join(".ULL").join("config").join("launcher.json")
}

/// 选后端：真实 host 优先；数据根解析不出来或显式 `--mock` 时退回假后端。
pub fn build_backend(options: &AppOptions) -> Box<dyn Backend> {
    if options.mock {
        return Box::new(MockBackend::new());
    }
    match &options.data_root {
        Some(root) => Box::new(CoreHost::new(HostEnv::at_root(root))),
        None => match CoreHost::open() {
            Some(host) => Box::new(host),
            None => Box::new(MockBackend::new()),
        },
    }
}
