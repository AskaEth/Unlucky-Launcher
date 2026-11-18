//! 「Java」分组的截图种子（M175，`ULL_JAVA_STATE`）：**只给 `--screenshot` 用**。
//!
//! 三档对应任务书要的三张图：未安装 Java 的空态 / 检测到两个 JDK 并选中其一 / 内存与 JVM 参数
//! 的编辑态。编辑态只让**内存**越界（`validate()` 一次只报第一个错，所以图上看到的是「内存超出
//! 允许范围」+ 内存框标红；引号那条另有单测，不靠这张图证明）。
//!
//! M197 加了三档，对应「下载一个 JDK」的三态（任务书要的三张图）：`download`（版本选择列表）/
//! `downloading`（下载中：百分比 + 当前源 + 一次退避）/ `download-failed`（失败：最后试到的源）。
//! 三档都把三个设置键摆成生产上真会出现的值（自定义落点 + 自己排的发行版顺序 + 自动准备开着），
//! 于是一张图里能同时看到「版本目录」与「三个键」。
//!
//! 与 `ULL_AI_STATE` / `ULL_UPDATE_STATE` / `ULL_FONT_STATUS` 同一族取证开关：只影响这一次进程，
//! 产品路径不设它。种子里的路径是**写死的示例路径**（真实机器上没有），所以它只在出图通道里出现；
//! 产品路径的真值一律来自 `core::game::java::staged` 的真扫盘。
//!
//! M204 r1（视觉复审 p2-1）又补了三档：`scanning`（正在扫描… 3/7，「取消」可点）/ `unscanned`
//! （还没扫描过）/ `cancelling`（正在取消…，两颗按钮都灰）。起因是六张举证图里「取消」全程是
//! 禁用态，「扫描中」一格都没拍，而 README 却写成「两态」——**证据覆盖不到结论**（M201 同一个坑）。
//! 这三档都把快照写进 `ScanSlot`（`store::ScanSlot::seed_report`），理由见那个函数的文档。

use super::state::{Context, Layer, Notice, Refusal, Requirement, RuntimeRow, State};

/// 取证开关 `ULL_JAVA_STATE`（只给 `--screenshot` 用）。
///
/// 三档对应三张图：`empty`（未检测到）/ `two`（两个 JDK 并选中其一）/ `editing`（改了内存与
/// JVM 参数、还没保存）。与 `ULL_AI_STATE` / `ULL_UPDATE_STATE` 同一族，只影响这一次进程。
#[must_use]
pub fn build(raw: &str) -> Option<State> {
    let mut state = State {
        storage: true,
        scanned: true,
        ..State::default()
    };
    match raw.trim() {
        "empty" => {
            state.memory = "2048".to_string();
            state.saved_memory = 2048;
        }
        "two" => {
            state.runtimes = vec![
                RuntimeRow {
                    binary: "C:/Program Files/Eclipse Adoptium/jdk-17.0.13/bin/java.exe"
                        .to_string(),
                    version: "openjdk version \"17.0.13\" 2024-10-15".to_string(),
                    major: Some(17),
                    vendor: Some("Temurin".to_string()),
                    source: "system",
                },
                RuntimeRow {
                    binary: "C:/Program Files/Microsoft/jdk-21.0.4/bin/java.exe".to_string(),
                    version: "openjdk version \"21.0.4\" 2024-07-16 LTS".to_string(),
                    major: Some(21),
                    vendor: Some("Microsoft".to_string()),
                    source: "system",
                },
            ];
            state.selected = state.runtimes[1].binary.clone();
            state.manual = vec!["D:/jdk/zulu-21".to_string()];
            state.memory = "4096".to_string();
            state.saved_memory = 4096;
            state.jvm = "-XX:+UseG1GC".to_string();
            state.saved_jvm = "-XX:+UseG1GC".to_string();
            state.context = Context {
                required: Some(Requirement {
                    instance: "生存服".to_string(),
                    version: "1.20.1".to_string(),
                    minimum: 17,
                    maximum: None,
                }),
                layer: Some(Layer::Global),
                effective_memory_mb: Some(4096),
            };
        }
        "editing" => {
            let mut seeded = build("two")?;
            seeded.memory = "128".to_string();
            seeded.jvm = "-XX:+UseG1GC -Xss\"512k".to_string();
            seeded.dirty = true;
            seeded.notice = Some(Notice::Refused(Refusal::MemoryRange));
            return Some(seeded);
        }
        // ── M204 r1（视觉复审 p2-1）：扫描的三格。复审指出六张图里「取消」全程是禁用态、
        // 「正在扫描… 3/7」一张没有，而 README 却写成了「两态」——这里补上能画出来的三格
        // （扫描中 / 还没扫描过 / 正在取消…；「扫描已取消…」那一句仍没有图，见 README §3）。
        //
        // 三档都把快照**写进槽**（`ScanSlot::seed_report`）而不只是改 `State.scanning`：
        // 心跳每拍用槽里的快照重算 `scanning`，只改 `State` 的话第一拍就被推回「没在扫」。
        "scanning" => {
            let mut seeded = build("two")?;
            // 扫描中：列表还没有结论（`scanned` 仍是 false），状态行画「正在扫描… 3/7」，
            // 「立即扫描」灰掉、「取消」可点（`view.rs` 的 `scanning && !cancelled`）。
            seeded.scanned = false;
            seeded.runtimes.clear();
            seeded.cancelled = false;
            seeded.scanning = Some(super::state::Progress { done: 3, total: 7 });
            seeded.scan.seed_report(super::store::ScanReport {
                running: true,
                done: 3,
                total: 7,
                finished: false,
                cancelled: false,
                runtimes: Vec::new(),
            });
            return Some(seeded);
        }
        "unscanned" => {
            // 还没扫描过：状态行「还没扫描过」，列表处画「列表还是空的…」（`view.rs:110-113`）。
            let mut seeded = build("empty")?;
            seeded.scanned = false;
            seeded.scanning = None;
            seeded.cancelled = false;
            seeded.scan.seed_report(super::store::ScanReport::default());
            return Some(seeded);
        }
        "cancelling" => {
            // 取消已按下、线程还没退出：状态行「正在取消…」、两颗按钮都灰（复审点名的另一格）。
            let mut seeded = build("scanning")?;
            seeded.cancelled = true;
            seeded.scan.seed_report(super::store::ScanReport {
                running: true,
                done: 3,
                total: 7,
                finished: false,
                cancelled: true,
                runtimes: Vec::new(),
            });
            return Some(seeded);
        }
        // ── M197：「下载一个 JDK」的三态
        "download" => {
            let mut seeded = build("two")?;
            seeded.context = download_context(21);
            seeded.prefs = seeded_prefs();
            seeded.effective = seeded_effective();
            return Some(seeded);
        }
        "downloading" => {
            let seeded = build("download")?;
            seeded.downloads.seed_report(super::downloads::Report {
                job: Some(super::downloads::Job {
                    major: 21,
                    phase: super::downloads::Phase::Downloading,
                    downloaded: 96_468_992,
                    total: Some(210_763_776),
                    source: Some("tuna".to_string()),
                    last: Some(super::downloads::Event::Retry {
                        source: "tuna".to_string(),
                        kind: unlucky_core::download::DownloadErrorKind::ServerError,
                        attempt: 2,
                        retry_in_ms: 2_000,
                    }),
                }),
                tries: vec![super::downloads::Try {
                    source: "tuna".to_string(),
                    kind: unlucky_core::download::DownloadErrorKind::ServerError,
                }],
                failure: None,
                success: None,
            });
            return Some(seeded);
        }
        "download-failed" => {
            let seeded = build("download")?;
            seeded.downloads.seed_report(super::downloads::Report {
                job: None,
                // 出图要能看见「试过哪些源、各自为什么」：两条真实形态的失败。
                tries: vec![
                    super::downloads::Try {
                        source: "tuna".to_string(),
                        kind: unlucky_core::download::DownloadErrorKind::NotFound,
                    },
                    super::downloads::Try {
                        source: "github".to_string(),
                        kind: unlucky_core::download::DownloadErrorKind::Timeout,
                    },
                ],
                success: None,
                failure: Some(super::downloads::Failure {
                    major: 21,
                    kind: unlucky_core::game::java::JavaErrorKind::Network,
                    source: Some("github".to_string()),
                }),
            });
            return Some(seeded);
        }
        _ => return None,
    }
    Some(state)
}

/// 出图用的「当前实例要求」（`1.20.5` 起要 21）。
fn download_context(minimum: u32) -> Context {
    Context {
        required: Some(Requirement {
            instance: "生存服".to_string(),
            version: "1.20.5".to_string(),
            minimum,
            maximum: None,
        }),
        layer: Some(Layer::Global),
        effective_memory_mb: Some(4096),
    }
}

/// 出图用的三键：自定义落点 + 自己排的顺序（第一行不是默认的那家）+ 自动准备开着。
fn seeded_prefs() -> super::prefs::Prefs {
    super::prefs::Prefs {
        install_root: "D:/Java".to_string(),
        saved_install_root: "D:/Java".to_string(),
        auto_install: true,
        saved_auto_install: true,
        priority: vec!["microsoft", "temurin", "oracle", "zulu"],
        saved_priority: vec!["microsoft", "temurin", "oracle", "zulu"],
    }
}

/// 出图用的生效值（与 [`seeded_prefs`] 一致：落点与顺序都来自设置）。
fn seeded_effective() -> super::prefs::Effective {
    super::prefs::Effective {
        root: "D:/Java".to_string(),
        root_mode: "custom",
        root_layer: super::prefs::RootLayer::Setting,
        root_unresolved: false,
        order: vec![
            "Microsoft OpenJDK",
            "Eclipse Temurin",
            "Oracle OpenJDK",
            "Azul Zulu",
        ],
        order_layer: super::prefs::OrderLayer::Setting,
        domestic: true,
    }
}
