//! 拦下对话框里那个「下载 Java {major}」出口的 host 侧（M334）。
//!
//! ## 为什么不是一套新的下载器
//!
//! 设置页「Java 运行时」那一条**已经是**完整的下载通道：core 的 `java::install` 跑四家发行版的
//! 回退链，过程与结局经 `HostTask::Java(JavaPageView)` → `CoreEvent::JavaChanged` 回到界面
//! （`host::java::spawn_install`）。这里把它复用成**同一个** `java::install`、**同一条**回包事件，
//! 一个字节的下载逻辑都不重写——「装一个 JDK」在进程里只该有一种说法。
//!
//! ## 与设置页那一条唯一的区别：大版本从哪来
//!
//! 设置页装的是 `java::target_major`：**第一条实例**要求什么就装什么。这对「用户点开某个实例
//! 却发现 Java 不符」这个现场是错的——1.12.2 + Forge 要 Java 8，而列表里第一条是 1.20.1 时要 17，
//! 装完 17 那个 1.12.2 照样起不来，对话框里那句「下载 Java 8」就是空话。所以这里的大版本由
//! **被拦下的那一条实例**的要求给（`LaunchBlockedView::install_major`），由调用方算好传进来。
//!
//! ## 结局怎么回：**链**是唯一的事实，`message` 一律留空
//!
//! 三步，全部走既有事件（不新开事件类型）：
//!
//! | 时刻 | 快照 | 对话框据此画 |
//! |---|---|---|
//! | 线程一起来 | `busy = true` | 「正在下载 Java {major}…」 |
//! | 装完 | `busy = false`、链里有一跳 `Hit` | 「已装好，可以重新启动」 |
//! | 装失败 | `busy = false`、链上写着哪一家为什么 | 具名原因（厂商名走目录 + core 原文） |
//!
//! `JavaPageView.message` 是 presenter 里的中文串（设置页在用），这里**一个字都不写**：
//! 英文界面上冒出一句 presenter 自写的中文就是同屏两语。失败原因全部由 `chain` 带出来。
//!
//! ## 安装编排**只在 core 一处**（M334 / 复审 r2 P2-1）
//!
//! 这里跟设置页一样，只调**一次** `java::install(InstallRequest::new(major))`：四家谁先谁后、
//! 跳过谁、Termux 上先试系统包还是先下归档，全部由 core 的回退链决定（`core/src/game/java/
//! install.rs`）。失败时从 `JavaError::chain()` 拿**逐家结构**（哪一家、什么原因），再翻成 DTO。
//!
//! 早先这里自己按 `order(..)` 逐家调过 `java::install`（为了在失败时留住每一家的原因），
//! 那等于把回退链**又写了一遍**：桌面平台结论一样，但 Termux 上设置页走系统包、对话框走归档
//! 下载，而且 core 将来改回退链只会改到一处。现在 core 的错误自己带着链，这条重复没有了
//! （`the_install_orchestration_lives_in_core_only` 守着它：presenter 里出现 `with_vendor` /
//! `order(..)` / `supports(..)` 就红）。

use unlucky_core::game::java::{self, InstallRequest};

use crate::dto::{JavaChainOutcome, JavaChainStep, JavaPageView};

use super::super::env::HostEnv;
use super::super::job::{spawn, HostTask, JobSender};

/// 装一步的结局：**回退链的每一跳**（顺序 = 先试谁）。
///
/// 失败也返回链（每一家各有一步），`spawn_install_with` 原样塞进 `JavaPageView.chain`。
/// 做成函数指针只为**让用例不联网**（见文件尾的 `test_hook`）。
pub type InstallStep = fn(&HostEnv, u32) -> Vec<JavaChainStep>;

/// 链上「还没走到任何一家」的那一跳的 `vendor`（空串）：安装位置解析不出来这类本地问题。
///
/// 空串是「没有发行版可点名」的天然写法——界面那一侧按它取「本机环境」那半句文案
/// （`app/dialog/model.rs::vendor_label`），与发行版名分开。
pub const LOCAL_STEP: &str = "";

/// 装一个**指定大版本**的托管 JDK（拦下对话框的第三个出口）。
pub fn spawn_install_major(env: HostEnv, tx: JobSender, major: u32) {
    spawn_install_with(env, tx, major, step_for_tests());
}

/// 同上，但「下载」那一步可注入（用例传自己的 [`InstallStep`]）。
pub fn spawn_install_with(env: HostEnv, tx: JobSender, major: u32, step: InstallStep) {
    spawn("java-install-launch", move || {
        // 先发「正在装」：界面的进展行靠它点亮（与设置页同一个 `busy` 口径）。
        let _ = tx.send(HostTask::Java(Box::new(snapshot(&env, Vec::new(), true))));
        let chain = step(&env, major);
        let _ = tx.send(HostTask::Java(Box::new(snapshot(&env, chain, false))));
    });
}

/// 真的走 core 的下载通道：**只调一次** `java::install`，回退链在 core 里面。
///
/// 一家都不支持这个大版本时**一次网络都不打**（core 的 `supports` 是纯判据）：core 记四条
/// 「跳过」并回错，链上照旧逐家可读，界面据此说「没有一家提供 Java {major}」。
pub(super) fn real_step(env: &HostEnv, major: u32) -> Vec<JavaChainStep> {
    let root = match super::super::engine::java_install_root(env) {
        Ok(root) => root,
        Err(reason) => {
            // 连落点都解析不出来（Standard 模式下拿不到系统目录这类）：还没走到任何一家。
            return vec![local_step(reason)];
        }
    };
    let engine = super::super::engine::engine(env);
    let priority = super::super::engine::java_priority(env);
    let request = InstallRequest::new(major).with_priority(priority);
    match unlucky_core::download::blocking::block_on(java::install(&engine, &root, request)) {
        Ok(installed) => super::super::java::chain_steps(&installed.chain),
        // 失败时 core 把**逐家试过的记录**挂在错误上（`JavaError::chain()`）：那是「哪一家、
        // 为什么」的唯一来源。拿不到链（还没走到任何一家）时才降级成一条「本机环境」。
        Err(error) => match error.chain() {
            Some(chain) => {
                let steps = super::super::java::chain_steps(chain);
                if steps.is_empty() {
                    vec![local_step(error.message().to_string())]
                } else {
                    steps
                }
            }
            None => vec![local_step(error.message().to_string())],
        },
    }
}

/// 链上「还没走到任何一家」的那一跳：`vendor` 是空串，`detail` 是 core/io 的原文（不翻译）。
fn local_step(reason: String) -> JavaChainStep {
    JavaChainStep {
        vendor: LOCAL_STEP.to_string(),
        outcome: JavaChainOutcome::Failed,
        detail: reason,
    }
}

/// 一份 Java 页快照（与设置页同一个构造函数，字段口径不会漂）。
///
/// `message` **恒空**：见文件头的「结局怎么回」。
fn snapshot(env: &HostEnv, chain: Vec<JavaChainStep>, busy: bool) -> JavaPageView {
    super::super::java::page(env, chain, busy, "")
}

#[cfg(test)]
pub(crate) fn set_install_step_for_tests(step: Option<InstallStep>) {
    test_hook::set(step);
}

#[cfg(test)]
fn step_for_tests() -> InstallStep {
    test_hook::get().unwrap_or(real_step)
}

#[cfg(not(test))]
fn step_for_tests() -> InstallStep {
    real_step
}

/// 用例替换「下载」那一步的钩子（**只能在本线程设**：`spawn_install_major` 在调用方线程取走
/// 函数指针再交给后台线程，所以线程局部的可见性正好够用）。
///
/// 为什么需要它：判据「点下去真的开始装了」必须穿过产品路径（`AppCommand::InstallJavaFor`
/// → 这条函数），而真装一个 JDK 要联网、几十兆、还可能写进用户的托管目录——单测绝不能这么干。
#[cfg(test)]
mod test_hook {
    use std::cell::RefCell;

    use super::InstallStep;

    thread_local! {
        static OVERRIDE: RefCell<Option<InstallStep>> = const { RefCell::new(None) };
    }

    /// 换掉下一步（`None` = 用真的）。
    pub(super) fn set(step: Option<InstallStep>) {
        OVERRIDE.with(|slot| *slot.borrow_mut() = step);
    }

    pub(super) fn get() -> Option<InstallStep> {
        OVERRIDE.with(|slot| *slot.borrow())
    }
}
