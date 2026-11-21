//! M304 用例的**共用夹具**（临时数据根 / 实例档案 / 首份快照落地 / 事件读取）。
//!
//! 拆出来的理由同仓里其它 `tests*` 的拆分：单文件 400 行硬线。判据本身在
//! [`super::target_tests`]（快照解析 / 同一拍 / 同名不猜 / 停止读 pid）与
//! [`super::revalidate_tests`]（所属根不见了 / 运行时态落进快照）。

use std::time::Duration;

use unlucky_core::game::install::instance::write_profiles;
use unlucky_core::game::{InstanceProfile, ProfileId, ProfileList};

use crate::dto::{AppEvent, Backend, CoreEvent};
use crate::toast::ToastKind;

use super::super::{CoreHost, HostEnv};

/// 临时数据根守卫：用例结束（含断言 panic）时删掉，不留空壳。
pub(super) struct TempRoot(pub(super) std::path::PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 一份**不存在**的 java 路径（让 `probe` 失败，落到「按版本要求兜底」那条路上）。
fn no_such_java() -> std::path::PathBuf {
    std::env::temp_dir()
        .join("ull-no-such-java")
        .join("bin")
        .join("java")
}

/// 一条实例档案（自建实例：id 与名字由用例给）。
///
/// `java_path` **恒 `None`，而且钉它没有用**：presenter 的启动链一次都不读
/// `InstanceProfile.java_path`——挑 Java 用 `launch_args::resolved`，它读的是
/// `config/instances/<id>.json` 的 `launch.javaPath` 与全局 `java.selected_path`。
/// 要钉就钉**那份**（见 [`pin_java_path`]，它走产品写盘路径）；不需要钉的用例用
/// [`host_unpinned`]。
pub(super) fn profile(name: &str, version: &str) -> InstanceProfile {
    InstanceProfile {
        id: ProfileId::generate(),
        name: name.to_string(),
        version_id: version.to_string(),
        game_dir: None,
        running_dir: None,
        java_path: None,
        created_at: None,
        last_played_at: None,
        extra: std::collections::BTreeMap::new(),
    }
}

/// 把一条实例的 Java 路径**真的钉进实例设置**（产品写盘路径：`launch_args::apply` →
/// `config/instances/<id>.json` 的 `launch.javaPath`，与 GUI「启动参数 → Java 路径」同一个出口）。
///
/// 为什么要钉：M331 起「点启动」会先判 Java 与版本要求是否相符，不钉的话这些用例的结果就
/// 取决于开发机装的是哪一版 JDK。钉一个**探测必然失败**的路径之后有**两重**保证：
/// ① `pick_java` 走显式分支，探测失败 ⇒ 主版本落回版本要求 ⇒ 判下来相符；
/// ② 按 M331 的口径「用户钉过的 Java 算显式」（`decide` 一路 `Proceed`）。
///
/// 只有①也能让用例绿，但那是靠「兜底口径」——那一条改了口径就又飘了；②是口径本身。
/// **M334 之后②只认实例层这一份**（全局默认 `java.selected_path` 不算显式，见 core 的
/// `java::select::explicit_java`）——所以这里钉实例文件这条路恰好就是「算显式」的那一条。
fn pin_java_path(env: &HostEnv, instance: &InstanceProfile) {
    let patch = crate::dto::LaunchArgsPatch {
        java_path: Some(no_such_java().display().to_string()),
        ..crate::dto::LaunchArgsPatch::default()
    };
    super::super::launch_args::apply(env, instance.id.as_str(), &instance.version_id, &patch)
        .expect("把 javaPath 写进实例设置");
}

/// 把实例的版本本体写成一份**最小可装配**的清单（`versions/<id>/<id>.json`）。
///
/// 用途只有一个：让「装配能走到 `Launcher::spawn`」这件事在夹具里成立。判据里有条锚是
/// 「会话目录没有 `.jsonl`」，而**没有版本本体时它恒真**（装配在读清单那一步就失败，`spawn`
/// 永远调不到）——M331 复审 P2-2 实测到的假锚。有了本体，任何一条绕开拦截走到 `spawn` 的路径
/// 都会写会话事件流，那条锚才谈得上「会红」。
///
/// `main_class` 由调用方给：M348 之后 `<1.13` 的**上限只属于 LaunchWrapper 系加载器**，
/// 判据就在主类上（`net.minecraft.launchwrapper.Launch`），夹具必须能指定它。
pub(super) fn write_minimal_version(env: &HostEnv, version: &str, main_class: &str) {
    write_version_json(env, version, main_class, None);
}

/// 同上，但可以声明 `javaVersion.majorVersion`（M349 r2 的 P2-1 判据要它）。
///
/// 为什么要声明的下限：那条判据要复现「机器上有一份**正好符合**该版本的 Java」——只有清单
/// 把下限抬到那份 Java 上，`actual == 下限` 才成立（否则老版本原版的下限恒 8，没有 JDK 8 的
/// 机器上永远造不出「正好符合」）。
pub(super) fn write_version_json(
    env: &HostEnv,
    version: &str,
    main_class: &str,
    java_major: Option<u32>,
) {
    let path = env.game.version_json(version);
    std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
    let java = java_major.map_or_else(String::new, |major| {
        format!(r#","javaVersion":{{"majorVersion":{major}}}"#)
    });
    std::fs::write(
        &path,
        format!(r#"{{"id":"{version}","type":"release","mainClass":"{main_class}"{java}}}"#),
    )
    .expect("写版本本体");
}

/// 把实例清单**写进盘**（`profiles.json` 是「盘上有没有这个实例」的档案侧真相）。
///
/// 增删实例都走它：用例改的就是 host 看不见的那份盘面，快照因此真的会过期。
pub(super) fn write(env: &HostEnv, profiles: &[InstanceProfile]) {
    let list = ProfileList {
        profiles: profiles.to_vec(),
        ..ProfileList::default()
    };
    write_profiles(&env.layout, &list).expect("写 profiles.json");
}

/// 建一个只有这几条档案的数据根，返回 `(host, 守卫)`。
///
/// **每条档案的 `javaPath` 都会被钉**（[`pin_java_path`]）：这些用例判的是实例解析与状态机，
/// 不该随开发机装的是哪一版 JDK 变化。
pub(super) fn host(label: &str, profiles: &[InstanceProfile]) -> (CoreHost, TempRoot) {
    let (host, root) = host_unpinned(label, profiles);
    for instance in profiles {
        pin_java_path(host.env(), instance);
    }
    (host, root)
}

/// 同上，但**不钉** `javaPath`：Java 交给 `pick_java` 自动挑。
///
/// M331 的判据要用这个——它们要的正是「自动挑到的 Java 与版本要求不符」这个真实状态；
/// 钉过路径的实例按口径算「用户显式指定」，永远不拦。
pub(super) fn host_unpinned(label: &str, profiles: &[InstanceProfile]) -> (CoreHost, TempRoot) {
    let dir = temp_dir(label);
    let env = HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.config()).expect("config 目录");
    write(&env, profiles);
    (CoreHost::headless(env), TempRoot(dir))
}

/// **两个根**的数据根：配置根里几条档案 + 官方根（`<data>/official/.minecraft`）里一个能登记的版本。
///
/// `HostEnv::at_root` 默认**不做多根发现**（见 `env.rs::at_root` 的文档：截图/用例要确定性），所以
/// 这里在交给 host 之前显式把 `standard_game` 指过去——用例要的正是「实例属于**另一个**根」。
///
/// **不钉 `javaPath`**（与 [`host`] 不同）：这条夹具产出的磁贴是自动发现的 `ext-` 实例，它没有
/// `config/instances/<id>.json` 可钉；而用它的用例（所属根不见了）在实例解析那一步就被拒了，
/// 根本走不到 Java 判定。
pub(super) fn host_with_official(
    label: &str,
    profiles: &[InstanceProfile],
    version: &str,
) -> (CoreHost, TempRoot, std::path::PathBuf) {
    let dir = temp_dir(label);
    let mut env = HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.config()).expect("config 目录");
    write(&env, profiles);
    let official = dir.join("official").join(".minecraft");
    let version_dir = official.join("versions").join(version);
    std::fs::create_dir_all(&version_dir).expect("官方根的版本目录");
    std::fs::write(
        version_dir.join(format!("{version}.json")),
        format!(r#"{{"id":"{version}","mainClass":"net.minecraft.client.main.Main"}}"#),
    )
    .expect("写版本 JSON");
    env.standard_game = Some(official.clone());
    (CoreHost::headless(env), TempRoot(dir), official)
}

/// 一次性临时目录（`line!()` 让同一个文件里的两条用例不会撞）。
fn temp_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ull-m304-{label}-{}-{}",
        std::process::id(),
        line!()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// 让第一份快照落地（M302 的后台扫盘），并把这一批事件读走。
pub(super) fn land_snapshot(host: &mut CoreHost) {
    host.refresh_instances();
    host.pump_instances();
    let _ = host.drain_events();
}

/// 有没有发过「运行时变了」（受理成功 = 一条 `Busy`）。
pub(super) fn runtime_event_emitted(events: &[AppEvent]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            AppEvent::Core(CoreEvent::InstanceRuntimeChanged { .. })
        )
    })
}

/// 事件里唯一那条错误 Toast 的原文（拒绝必须是**具名**的，所以这里要求它存在）。
pub(super) fn error_toast(events: &[AppEvent]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            AppEvent::Toast(toast) if toast.kind == ToastKind::Error => Some(toast.text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("拒绝受理必须给一条错误提示（具名原因），实际事件：{events:?}"))
}

/// 等启动后台线程把结局交回来。
///
/// 调用它的用例都不写版本本体 ⇒ 装配必然在读清单那一步失败，**不会真起进程**；只有判据①
/// （[`super::guard_tests`]）特意写了一份最小版本本体，用来让「会话事件流」那条锚真的会红，
/// 而那条用例走的是**拦下**分支（不 `settle`）。
///
/// 不只是收尾：它顺手证明**受理不依赖这条线程**——Busy 的断言都在它之前做完了。
pub(super) fn settle(host: &mut CoreHost) {
    for _ in 0..2_000 {
        if host.pending.is_empty() {
            return;
        }
        host.tick(Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("启动线程没在 2 s 内收尾（pending 还剩 {:?}）", host.pending);
}

/// 会话事件流文件（`<sessions_root>/game/<session-id>.jsonl`）。
///
/// `Launcher::spawn` 一进去就 `SessionWriter::create` ⇒ 这里出现任何 `.jsonl` 就说明 **spawn 真的被
/// 调过**。只数 `.jsonl` 不数整棵目录：启动器日志（`launcher.log` / `session-log-location.json`）
/// 与它同一个根，拦下时**故意**要记一条「为什么没起」，那是另一件事。
pub(super) fn session_logs(host: &CoreHost) -> Vec<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                out.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&host.env().sessions_root(), &mut found);
    found.sort();
    found
}

/// 等到「有 spawn 的盘面证据」为止（或 `window` 用完），返回这期间收到的运行时事件。
///
/// 只给 M331 的判据①用：拦下这条路上**不填 `pending`**，所以 `settle()` 会立刻返回；而
/// 「绕开受理记账直接起进程」的路径是后台线程跑的——不等到它跑到 `spawn`，会话事件流与启动表
/// 都还是空的，那两条锚就是「断言得太早」的假绿。
///
/// 有动静就提前退出 ⇒ 判据不靠计时，`window` 只决定「正确的实现要等多久」。
pub(super) fn wait_while_no_spawn(host: &mut CoreHost, window: Duration) -> Vec<AppEvent> {
    let mut seen = Vec::new();
    let deadline = std::time::Instant::now() + window;
    while std::time::Instant::now() < deadline
        && session_logs(host).is_empty()
        && host.launches.is_empty()
    {
        host.tick(Duration::from_millis(5));
        seen.extend(host.drain_events());
        std::thread::sleep(Duration::from_millis(5));
    }
    seen
}
