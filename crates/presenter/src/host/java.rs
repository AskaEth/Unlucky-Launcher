//! Java 页：四家发行版 + 回退链可视化 + 本地运行时 + 托管 JDK 安装。
//!
//! 回退链的两态：
//! - **计划链**（还没装）：按生效顺序铺一排「待尝试」，让用户先看清会按什么顺序找；
//! - **真实链**（装过一次/装失败）：`core::game::java::ChainLog` 的每跳结局与原因。
//!
//! 安装走 `java::install`（core 自己跑回退链），host 只负责把 `ChainLog` 翻成 DTO。

use unlucky_core::game::java::probe::JavaOrigin;
use unlucky_core::game::java::source::ChainLog;
use unlucky_core::game::java::{self, JdkVendor};

use crate::dto::{JavaChainOutcome, JavaChainStep, JavaPageView, JavaRuntimeView, JavaVendorView};

use super::env::HostEnv;
use super::job::{spawn, HostTask, JobSender};

/// 托管 JDK 的默认目标大版本（D13 基准 JDK17）。
pub const DEFAULT_JDK_MAJOR: u32 = 17;

/// 该装哪个大版本：取第一个实例版本的要求，没有实例就给 17。
pub fn target_major(env: &HostEnv) -> u32 {
    let profiles = super::game::profiles(env);
    profiles
        .profiles
        .first()
        .map(|profile| {
            java::requirement_for_version(&env.game, &profile.version_id)
                .minimum
                .max(8)
        })
        .unwrap_or(DEFAULT_JDK_MAJOR)
}

/// 四家发行版的展示行（镜像与吞吐口径与 CLI `ull java vendors` 一致）。
pub fn vendors(env: &HostEnv, installed_vendors: &[String]) -> Vec<JavaVendorView> {
    let domestic = super::engine::domestic(env);
    let order = java::vendor::order(&super::engine::java_priority(env), domestic);
    let table = unlucky_core::download::MirrorTable::builtin();
    order
        .iter()
        .enumerate()
        .map(|(index, vendor)| {
            let id = vendor.id();
            JavaVendorView {
                id: id.to_string(),
                display: vendor.display().to_string(),
                rank: u32::try_from(index + 1).unwrap_or(u32::MAX),
                majors: vendor
                    .hint_majors()
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join("/"),
                sources: sources_of(&table, *vendor),
                installed: installed_vendors.iter().any(|known| known == id),
            }
        })
        .collect()
}

/// 一家的源清单（与 CLI 展示同形）。
fn sources_of(table: &unlucky_core::download::MirrorTable, vendor: JdkVendor) -> Vec<String> {
    match vendor {
        JdkVendor::Temurin => table
            .adoptium
            .iter()
            .filter(|mirror| mirror.enabled)
            .map(|mirror| mirror.name.clone())
            .chain(std::iter::once(
                "GitHub 官方（国内套 gh-proxy）".to_string(),
            ))
            .collect(),
        JdkVendor::Oracle => table
            .oracle_mirrors()
            .map(|mirror| format!("{} {}", mirror.name, mirror.base()))
            .collect(),
        JdkVendor::Microsoft => vec!["aka.ms → VS CDN（无国内镜像）".to_string()],
        JdkVendor::Zulu => table
            .zulu()
            .map(|zulu| vec![format!("{}（{}）", zulu.api, zulu.measured_speed)])
            .unwrap_or_default(),
        JdkVendor::Termux => unlucky_core::game::java::termux::MIRRORS
            .iter()
            .map(|mirror| format!("{mirror}（系统包：pkg install openjdk-<major>）"))
            .collect(),
    }
}

/// 本地运行时（托管 / JAVA_HOME / PATH / 系统安装位置，同一路径只留优先级高者）。
pub fn runtimes(env: &HostEnv) -> Vec<JavaRuntimeView> {
    let roots = super::engine::java_scan_roots(env);
    let candidates = java::discover::dedupe(java::candidates_in_roots(&roots));
    candidates
        .iter()
        .map(|runtime| JavaRuntimeView {
            binary: runtime.binary.display().to_string(),
            version: runtime
                .version
                .clone()
                .unwrap_or_else(|| "版本未知".to_string()),
            major: runtime.major.unwrap_or(java::DEFAULT_MAJOR),
            vendor: runtime.vendor.clone().unwrap_or_else(|| "—".to_string()),
            origin: runtime.origin.category().to_string(),
        })
        .collect()
}

/// 托管目录（展示「装到哪」）。
pub fn managed_roots(env: &HostEnv) -> Vec<String> {
    super::engine::java_scan_roots(env)
        .iter()
        .map(|root| root.display().to_string())
        .collect()
}

/// 计划链（按生效顺序铺「待尝试」）。
pub fn planned_chain(env: &HostEnv) -> Vec<JavaChainStep> {
    let domestic = super::engine::domestic(env);
    let order = java::vendor::order(&super::engine::java_priority(env), domestic);
    order
        .iter()
        .map(|vendor| JavaChainStep {
            vendor: vendor.id().to_string(),
            outcome: JavaChainOutcome::Pending,
            detail: String::new(),
        })
        .collect()
}

/// 真实链（`ChainLog` → DTO）。
pub fn chain_steps(log: &ChainLog) -> Vec<JavaChainStep> {
    log.attempts()
        .iter()
        .map(|attempt| JavaChainStep {
            vendor: attempt.vendor.id().to_string(),
            outcome: if attempt.ok {
                JavaChainOutcome::Hit
            } else if attempt.detail.contains("不支持") {
                JavaChainOutcome::Skip
            } else {
                JavaChainOutcome::Failed
            },
            detail: attempt.detail.clone(),
        })
        .collect()
}

/// 整页快照。
pub fn page(env: &HostEnv, chain: Vec<JavaChainStep>, busy: bool, message: &str) -> JavaPageView {
    let runtimes = runtimes(env);
    let installed_vendors: Vec<String> = runtimes
        .iter()
        .filter(|runtime| runtime.origin == JavaOrigin::Managed.category())
        .map(|runtime| runtime.vendor.clone())
        .collect();
    let vendor_rows = vendors(env, &installed_vendors);
    let domestic = super::engine::domestic(env);
    let priority_set = !super::engine::java_priority(env).is_empty();
    let chain = if chain.is_empty() {
        planned_chain(env)
    } else {
        chain
    };
    JavaPageView {
        region: if domestic { "domestic" } else { "global" }.to_string(),
        priority_source: if priority_set {
            "java.vendorPriority"
        } else {
            "region_default"
        }
        .to_string(),
        order_text: vendor_rows
            .iter()
            .map(|vendor| vendor.display.clone())
            .collect::<Vec<_>>()
            .join(" → "),
        vendors: vendor_rows,
        chain,
        runtimes,
        managed_roots: managed_roots(env),
        busy,
        message: message.to_string(),
    }
}

/// 后台装一个托管 JDK（`vendor` 为空 = 按回退链自己选）。
pub fn spawn_install(env: HostEnv, tx: JobSender, vendor: Option<JdkVendor>) {
    spawn("java-install", move || {
        let major = target_major(&env);
        let mut busy = page(&env, Vec::new(), true, &format!("正在安装 Java {major}…"));
        busy.busy = true;
        let _ = tx.send(HostTask::Java(Box::new(busy)));

        let engine = super::engine::engine(&env);
        let root = match super::engine::java_install_root(&env) {
            Ok(root) => root,
            Err(reason) => {
                let _ = tx.send(HostTask::Java(Box::new(page(
                    &env,
                    Vec::new(),
                    false,
                    &format!("解析 Java 安装位置失败：{reason}"),
                ))));
                return;
            }
        };
        let request = match vendor {
            Some(vendor) => java::InstallRequest::with_vendor(major, vendor)
                .with_priority(super::engine::java_priority(&env)),
            None => {
                java::InstallRequest::new(major).with_priority(super::engine::java_priority(&env))
            }
        };
        let result =
            unlucky_core::download::blocking::block_on(java::install(&engine, &root, request));
        match result {
            Ok(installed) => {
                let message = if installed.reused() {
                    format!(
                        "Java {}（{}）已就绪：{}",
                        installed.major,
                        installed.vendor.id(),
                        installed.root.display()
                    )
                } else {
                    format!(
                        "已安装 Java {}（{}）→ {} · {} 个文件",
                        installed.major,
                        installed.vendor.display(),
                        installed.root.display(),
                        installed.files
                    )
                };
                let chain = chain_steps(&installed.chain);
                let _ = tx.send(HostTask::Java(Box::new(page(&env, chain, false, &message))));
            }
            Err(error) => {
                let _ = tx.send(HostTask::Java(Box::new(page(
                    &env,
                    Vec::new(),
                    false,
                    &format!("Java 安装失败：{error}"),
                ))));
            }
        }
    });
}

/// 「安装一个 Java」的 UI 文案（下载页 Java 分组的按钮说明）。
pub fn install_hint(env: &HostEnv) -> String {
    let major = target_major(env);
    format!("按回退链取一个 Java {major}（托管目录，装完自动被启动链选中）")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_chain_covers_four_vendors() {
        let dir = std::env::temp_dir().join(format!("ull-host-java-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let env = HostEnv::at_root(&dir);
        std::fs::create_dir_all(env.layout.config()).expect("config");
        let chain = planned_chain(&env);
        assert_eq!(chain.len(), 4);
        assert!(chain
            .iter()
            .all(|step| step.outcome == JavaChainOutcome::Pending));
        assert_eq!(chain[0].vendor, "temurin", "国内/国外默认都是 Temurin 第一");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn chain_log_maps_to_steps() {
        let mut log = ChainLog::new();
        log.skip(
            JdkVendor::Microsoft,
            "不支持 Java 8（microsoft 只发 11/17/21/25）",
        );
        log.hit(JdkVendor::Temurin, "命中 Adoptium 17");
        let steps = chain_steps(&log);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].outcome, JavaChainOutcome::Skip);
        assert_eq!(steps[1].outcome, JavaChainOutcome::Hit);
    }

    #[test]
    fn page_reports_region_and_priority_source() {
        let dir = std::env::temp_dir().join(format!("ull-host-java-page-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let env = HostEnv::at_root(&dir);
        std::fs::create_dir_all(env.layout.config()).expect("config");
        std::fs::write(
            env.launcher_config(),
            r#"{"java":{"vendorPriority":["zulu"]}}"#,
        )
        .expect("写配置");
        let page = page(&env, Vec::new(), false, "");
        assert_eq!(page.priority_source, "java.vendorPriority");
        assert_eq!(page.vendors[0].id, "zulu");
        assert_eq!(page.vendors[0].rank, 1);
        assert!(page.order_text.contains("Azul Zulu"));
        assert_eq!(page.vendors.len(), 4);
        std::fs::remove_dir_all(&dir).ok();
    }
}
