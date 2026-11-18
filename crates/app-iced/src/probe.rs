//! 渲染后端探测 + 自动降级（D27）。
//!
//! ## 为什么需要探测
//!
//! iced 0.14 的 `Renderer` 本身就是「wgpu → tiny-skia」的运行时回退枚举（M60 Q1）：
//! **不设** `ICED_BACKEND` 时，拿不到 GPU 适配器会自动降级到 CPU，零代码。
//! 需要自研的只有**判据**：
//!
//! - 本机会枚举出 `Microsoft Basic Render Driver [Cpu]`（= WARP 软光栅），
//!   所以「有没有适配器」不能当「有没有 GPU」（M60 实测），必须看 `device_type`。
//! - 没有硬件 GPU 时要**主动**把 CPU 放到候选首位，否则 wgpu 会先花时间尝试 WARP，
//!   观感与行为都难料。
//!
//! ## M97 订正：判据在 [`crate::renderer::decide`]，日志不再说谎
//!
//! 本模块只负责**探测**（`Probe::run`）和**落地**（[`apply`]）；决策表、兜底、日志格式
//! 全部搬到了 [`crate::renderer`] 的模块文档里（那张判据表就是验收标准）。
//!
//! 三条被删掉/改掉的东西，别再加回来：
//!
//! 1. **`ICED_BACKEND` 不再写候选列表**（`wgpu,tiny-skia` / `tiny-skia,wgpu` 这种）。
//!    列表语义让 iced 自己回退，而它的回退是静默的：`iced_renderer-0.14.0/src/fallback.rs`
//!    把 wgpu 的错误装进 `errors` 向量，tiny-skia 一成功就整体丢弃。现在只写**一项**，
//!    起不来就必然是具名错误，由 `app::boot` 打印并重开降级。
//! 2. **`auto` + 有硬件不再「不设变量」**。那等于把决策权交给一条我们看不见的路径。
//! 3. **`renderer = auto (wgpu -> tiny-skia)` 这行删了**。它是按「变量未设」打出来的常量，
//!    跟实际后端无关——M97 的 macOS 报告就是被它误导的。

use crate::renderer::{self, Decision};
use crate::Backend;

/// 单个适配器的摘要（只留日志要用的字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterSummary {
    /// 适配器名（如 `AMD Radeon RX 6600 XT`）。
    pub name: String,
    /// `device_type` 的字符串形式（`DiscreteGpu` / `IntegratedGpu` / `Cpu` / …）。
    pub device_type: &'static str,
}

/// 探测结论。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Probe {
    /// 枚举到的全部适配器（含 WARP 这类软光栅）。
    pub adapters: Vec<AdapterSummary>,
    /// 是否存在**真**硬件适配器（`device_type` 既不是 `Cpu` 也不是 `Other`）。
    pub hardware: bool,
}

impl Probe {
    /// 探测本机渲染能力（编入 `wgpu` 时真的枚举适配器）。
    #[cfg(feature = "gpu")]
    pub fn run() -> Self {
        run_wgpu()
    }

    /// 探测本机渲染能力（`--no-default-features` 的纯 CPU 构建：没有 wgpu，直接判定无硬件）。
    #[cfg(not(feature = "gpu"))]
    pub fn run() -> Self {
        Self::default()
    }

    /// 一行日志：适配器清单 + 判定。
    pub fn describe(&self) -> String {
        if self.adapters.is_empty() {
            return format!("adapters=0 hardware={}", self.hardware);
        }

        let list = self
            .adapters
            .iter()
            .map(|adapter| format!("{} [{}]", adapter.name, adapter.device_type))
            .collect::<Vec<_>>()
            .join(" | ");

        format!(
            "adapters={} [{list}] hardware={}",
            self.adapters.len(),
            self.hardware
        )
    }
}

/// 把 [`renderer::decide`] 的结论落到进程环境变量，并把**判据**打进日志（不带预检）。
///
/// 这个入口给「没有重试能力」的调用方（组件库 harness，`--screen=gallery`）用：它写完变量就
/// 直接 `iced::application(..).run()`，拿不到 `app/boot.rs` 的「换进程降级」。
pub fn apply(request: Backend, probe: &Probe) -> Decision {
    apply_with_preflight(request, probe, None)
}

/// 同上，但接受 [`renderer::preflight`] 的结果（`ULL_PREFLIGHT=1` 时才有值）。
///
/// 注意这里是「判据」不是「结果」：日志里不会出现「实际用了哪个后端」这种我们观测不到的话。
/// 实际后端的取证有两条，都在 [`crate::renderer`] 的判据表里：
/// ① `[gpu] iced_wgpu: …Selected: AdapterInfo…`（iced 自己在创建设备时打的）；
/// ② 起不来时的具名错误 + `app/boot.rs` 的重开降级。
///
/// 预检失败就把 wgpu 降下来（判据表第 7 行），免得为了拿一条错误去闪一次窗。
pub fn apply_with_preflight(
    request: Backend,
    probe: &Probe,
    preflight: Option<&Result<renderer::preflight::Verdict, renderer::preflight::Failure>>,
) -> Decision {
    let gpu_compiled = cfg!(feature = "gpu");
    let mut decision = renderer::decide(request, probe, gpu_compiled);

    if matches!(preflight, Some(Err(_))) {
        decision = renderer::demote_after_preflight(decision);
    }

    // ICED_BACKEND 是进程级环境变量，必须在 `iced::application(..).run()` 之前写。
    // 仓库是 edition 2021 → `set_var` 仍是安全 API（升 2024 后要改成子进程 exec 自身）。
    std::env::set_var("ICED_BACKEND", decision.primary.env_value());

    let note = if !gpu_compiled && request != Backend::Cpu {
        "；本构建未编入 wgpu（--no-default-features），只能 CPU"
    } else {
        ""
    };

    // M188 复审 P2-1：这三行是**一次性的启动取证**（后端请求 → 落定的 ICED_BACKEND → 取证指南），
    // 必须进 `<.ULL>/logs/launcher.log`：双击启动没有 stderr，否则「为什么用这个后端 / 为什么降级」
    // 在事故之后完全不可复原。
    crate::app::diag::diag!(
        "[probe] {} :: request={} -> ICED_BACKEND={}{note}",
        probe.describe(),
        request.name(),
        decision.primary.env_value()
    );
    crate::app::diag::diag!("[probe] {}", decision.describe());
    if decision.primary == renderer::Kind::Wgpu {
        crate::app::diag::diag!(
            "[probe] 取证：出现 `[gpu] iced_wgpu: …Selected: AdapterInfo…` ⇒ GPU 真生效；\
             出现 `[gpu] wgpu 初始化失败` ⇒ 已降级 CPU"
        );
    }

    decision
}

#[cfg(feature = "gpu")]
fn run_wgpu() -> Probe {
    // `iced::wgpu` 就是 wgpu crate 本身（`pub use iced_renderer::wgpu::wgpu` 再导出），
    // 所以不需要单独依赖 wgpu，版本也不会和 iced 打架。
    use iced::wgpu::{Backends, Instance, InstanceDescriptor};

    let instance = Instance::new(&InstanceDescriptor {
        backends: Backends::all(),
        ..Default::default()
    });

    let adapters = instance
        .enumerate_adapters(Backends::all())
        .into_iter()
        .map(|adapter| {
            let info = adapter.get_info();
            AdapterSummary {
                name: info.name,
                device_type: renderer::preflight::device_type_name(info.device_type),
            }
        })
        .collect::<Vec<_>>();

    let hardware = adapters
        .iter()
        .any(|adapter| !matches!(adapter.device_type, "Cpu" | "Other"));

    Probe { adapters, hardware }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(types: &[&'static str]) -> Probe {
        let adapters = types
            .iter()
            .map(|device_type| AdapterSummary {
                name: "adapter".to_owned(),
                device_type,
            })
            .collect::<Vec<_>>();
        Probe {
            adapters,
            hardware: types
                .iter()
                .any(|device_type| !matches!(*device_type, "Cpu" | "Other")),
        }
    }

    #[test]
    fn warp_only_machine_counts_as_no_gpu() {
        // 本机实测会枚举出 WARP：`Microsoft Basic Render Driver [Cpu]`
        let warp = probe(&["Cpu"]);
        assert!(!warp.hardware, "只有 WARP 不能当「有 GPU」");
        assert_ne!(
            renderer::decide(Backend::Auto, &warp, true).primary,
            renderer::Kind::Wgpu
        );
    }

    #[test]
    fn describe_lists_every_adapter() {
        let text = probe(&["DiscreteGpu", "Cpu"]).describe();
        assert!(text.contains("DiscreteGpu"), "{text}");
        assert!(text.contains("Cpu"), "{text}");
        assert!(text.contains("hardware=true"), "{text}");
    }

    /// 判据矩阵本身在 [`crate::renderer::decide`] 的测试里逐行钉死（表驱动 + 兜底守门）。
    /// 这里只守 `apply` 这一层的**落地**契约：写进环境变量的永远是**单项**。
    ///
    /// 抓得住：谁把候选列表（`wgpu,tiny-skia`）加回来——那会让 iced 的静默回退复活。
    /// 抓不住：iced 运行期真的选了谁（那是判据表第 8 行的事）。
    #[test]
    fn apply_writes_exactly_one_backend() {
        let gpu = probe(&["IntegratedGpu"]);
        for request in [Backend::Auto, Backend::Gpu, Backend::Cpu] {
            let decision = renderer::decide(request, &gpu, true);
            let value = decision.primary.env_value();
            assert!(
                !value.contains(','),
                "ICED_BACKEND 只许单项，收到 {value:?}"
            );
            assert!(matches!(value, "wgpu" | "tiny-skia"), "{value}");
        }
    }
}
