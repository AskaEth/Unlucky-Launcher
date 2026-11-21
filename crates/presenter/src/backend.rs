//! 后端选择与启动选项：真实 core host 优先，解析不出数据根时退回 mock。

use std::path::{Path, PathBuf};

use crate::dto::Backend;
use crate::host::CoreHost;
use crate::host::HostEnv;
use crate::mock::MockBackend;

/// 启动选项（`--data-dir` 与 `--mock` 是产品开关；截图/演示也复用同一份选项）。
#[derive(Debug, Clone, Default)]
pub struct AppOptions {
    /// 钉死数据根（`<root>/.ULL` + `<root>/.minecraft`）；None = 按便携配置解析。
    ///
    /// 走 [`HostEnv::at_data_root`]：**照常**发现官方启动器的标准游戏目录（M274）。钉数据根
    /// 不等于「别看官方实例」——那是两个不同的开关。
    pub data_root: Option<PathBuf>,
    /// 强制 mock 后端（UI 回归/无数据根演示）
    pub mock: bool,
}

/// `<root>/.ULL/config/launcher.json`（与 `SettingsBridge::open` 同落点）。
pub fn config_path(root: &Path) -> PathBuf {
    root.join(".ULL").join("config").join("launcher.json")
}

/// 钉了数据根时用哪个环境（`None` = 没钉，走解析链）。
///
/// **单独一格只为可测**：`--data-dir` 是产品通道，钉了数据根照样要看见官方启动器里的实例
/// （M274 r1 复审 P2-2）。退回 [`HostEnv::at_root`] 会让用户**静默**丢一半实例，而这层
/// `Box<dyn Backend>` 在用例里没有 env 取回口（`Backend` 没有访问器），所以判据就落在这个
/// 纯函数上（`backend::tests::data_root_keeps_discovering_the_official_launcher`）。
fn host_env_for(options: &AppOptions) -> Option<HostEnv> {
    options.data_root.as_ref().map(HostEnv::at_data_root)
}

/// 选后端：真实 host 优先；数据根解析不出来或显式 `--mock` 时退回假后端。
pub fn build_backend(options: &AppOptions) -> Box<dyn Backend> {
    if options.mock {
        return Box::new(MockBackend::new());
    }
    match host_env_for(options) {
        Some(env) => Box::new(CoreHost::new(env)),
        None => match CoreHost::open() {
            Some(host) => Box::new(host),
            None => Box::new(MockBackend::new()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--data-dir`（产品通道）**保留**多根发现；`at_root`（截图/演示/用例）不保留。
    ///
    /// 抓得住：`host_env_for` 被改回 `HostEnv::at_root`——用户钉一个数据根就永久静默看不到
    /// 官方启动器里的实例（M274 r1 复审 P2-2）。
    /// 抓不住：`--data-dir` 的命令行解析（app 侧）；mounted 的数据根真的解析得出来（那是 resolver）。
    #[test]
    fn data_root_keeps_discovering_the_official_launcher() {
        let root = std::env::temp_dir().join(format!("ull-backend-m274-{}", std::process::id()));
        let options = AppOptions {
            data_root: Some(root.clone()),
            mock: false,
        };
        let env = host_env_for(&options).expect("钉了数据根就该有环境");
        assert_eq!(
            env.game.root(),
            root.join(".minecraft"),
            "落点与 `at_root` 一致"
        );
        assert_eq!(
            env.standard_game,
            unlucky_core::platform::paths::standard_game_home(
                unlucky_core::platform::paths::Platform::current(),
                &unlucky_core::platform::paths::ProcessEnv,
            ),
            "`--data-dir` 是产品通道：不许顺手关掉官方根发现"
        );
        // 没钉数据根时这一格不参与（走解析链），`mock` 在前一步就分走了。
        assert!(host_env_for(&AppOptions::default()).is_none());
        std::fs::remove_dir_all(&root).ok();
    }
}
