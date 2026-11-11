//! wgpu 自检（判据表第 7 行）：在把控制权交给 iced **之前**自己跑一遍
//! 「实例 → 适配器 → 设备」，把 wgpu 的具名错误拿在手里。
//!
//! ## 为什么默认关
//!
//! 它会**重复** iced 马上要做的活（再建一个实例、再要一个设备），macOS 上一个 Metal 设备
//! 不是免费的；而且它测不了 `create_surface`（那时还没有窗口），所以它**代替不了**真跑一次。
//! 它的价值只有一个：让「不上 GPU」这个结论在**开窗之前**就有原因，验收时不用盯着一个闪窗猜。
//!
//! 开关：`ULL_PREFLIGHT=1`（判据表里登记过，不是隐藏口子）。
//!
//! ## 它镜像的是谁
//!
//! 逐字对着 `iced_wgpu-0.14.0/src/window/compositor.rs` 的 `Compositor::request()` 抄的：
//! `Backends::from_env().unwrap_or(all)`、`PowerPreference::from_env().unwrap_or(HighPerformance)`、
//! `Limits { max_bind_groups: 2, max_non_sampler_bindings: 2048, ..default }`。
//! 差一处都会让预检和真实路径说不同的话，那时预检就不是证据而是噪声了。

/// 预检开关的环境变量名（`1` / `true` 打开）。
pub const ENV_SWITCH: &str = "ULL_PREFLIGHT";

/// 预检成功：拿到的适配器摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// 适配器名（`Apple M1` / `AMD Radeon RX 6600 XT` / `Microsoft Basic Render Driver`）。
    pub adapter: String,
    /// `device_type` 的字符串形式。
    pub device_type: &'static str,
    /// 命中哪个 wgpu 后端（`Metal` / `Vulkan` / `Dx12` / `Gl`）。
    pub backend: String,
}

impl Verdict {
    /// 一行日志。
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "ok adapter={:?} type={} backend={}",
            self.adapter, self.device_type, self.backend
        )
    }
}

/// 预检失败：卡在哪一步 + wgpu 自己的错误文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// 失败阶段（日志里 `stage=` 的取值）。
    pub stage: &'static str,
    /// wgpu 具名错误（`Display`，原样保留，不翻译成「失败」）。
    pub detail: String,
}

impl Failure {
    /// 一行日志（`stage=` 可直接 grep）。
    #[must_use]
    pub fn describe(&self) -> String {
        format!("FAILED stage={} error={}", self.stage, self.detail)
    }
}

/// 用户是否要求预检。
#[must_use]
pub fn requested() -> bool {
    std::env::var(ENV_SWITCH).is_ok_and(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
}

/// 按 `ULL_PREFLIGHT` 决定跑不跑；跑了就顺手把结论打进 stderr（带耗时，别让开关成本变成传说）。
///
/// 返回 `None` = 没要求预检（调用方按判据表 1~6 行原样走）。
#[must_use]
pub fn run_if_requested() -> Option<Result<Verdict, Failure>> {
    if !requested() {
        return None;
    }

    let started = std::time::Instant::now();
    let verdict = run();
    let took = started.elapsed().as_secs_f64() * 1000.0;
    // M188：显式开关（`ULL_PREFLIGHT=1`）下的一次性行，按 `app::diag` 的判据可以进
    // `launcher.log`——用户开这个开关就是为了拿到「为什么起不来」的证据。
    match &verdict {
        Ok(ok) => {
            crate::app::diag::diag!("[gpu-preflight] {} took={took:.0}ms", ok.describe());
        }
        Err(failure) => {
            crate::app::diag::diag!("[gpu-preflight] {} took={took:.0}ms", failure.describe());
        }
    }
    Some(verdict)
}

#[cfg(feature = "gpu")]
/// 真跑一遍 wgpu 初始化（不开窗、不建 surface）。
///
/// # Errors
/// 无适配器 / 设备请求被拒时返回 [`Failure`]，`detail` 是 wgpu 的原话。
pub fn run() -> Result<Verdict, Failure> {
    use iced::wgpu::{
        Backends, ExperimentalFeatures, Features, Instance, InstanceDescriptor, Limits,
        MemoryHints, PowerPreference, RequestAdapterOptions, Trace,
    };

    let backends = Backends::from_env().unwrap_or_else(Backends::all);
    let instance = Instance::new(&InstanceDescriptor {
        backends,
        ..Default::default()
    });

    let options = RequestAdapterOptions {
        power_preference: PowerPreference::from_env().unwrap_or(PowerPreference::HighPerformance),
        // 没有窗口，测不了 surface 兼容性——这一条只能靠真跑一次（判据表第 8 行）。
        compatible_surface: None,
        force_fallback_adapter: false,
    };

    let adapter = block_on(instance.request_adapter(&options)).map_err(|error| Failure {
        stage: "request-adapter",
        detail: format!("{options:?}: {error}"),
    })?;

    let info = adapter.get_info();
    let device_type = device_type_name(info.device_type);

    // iced 的 `request_device` 失败时不打日志、只把错误塞进 `RequestDeviceFailed`，
    // 所以我们在这里先请求一次，把它的话原样抄下来。
    let _ = block_on(adapter.request_device(&iced::wgpu::DeviceDescriptor {
        label: Some("unlucky-launcher gpu preflight"),
        required_features: Features::empty(),
        required_limits: Limits {
            max_bind_groups: 2,
            max_non_sampler_bindings: 2048,
            ..Limits::default()
        },
        memory_hints: MemoryHints::MemoryUsage,
        trace: Trace::Off,
        experimental_features: ExperimentalFeatures::disabled(),
    }))
    .map_err(|error| Failure {
        stage: "request-device",
        detail: error.to_string(),
    })?;

    Ok(Verdict {
        adapter: info.name,
        device_type,
        backend: format!("{:?}", info.backend),
    })
}

#[cfg(not(feature = "gpu"))]
/// 本构建没编 wgpu：预检必然是「不可用」，但这不是错误——判据表第 4 行已经说明原因。
///
/// # Errors
/// 恒返回 [`Failure`]（`stage=not-compiled`）。
pub fn run() -> Result<Verdict, Failure> {
    Err(Failure {
        stage: "not-compiled",
        detail: "本构建未编入 wgpu（--no-default-features）".to_owned(),
    })
}

#[cfg(feature = "gpu")]
/// `DeviceType` → 日志里的字符串（探测与预检**共用**这一处，别各写一份 `match`）。
#[must_use]
pub fn device_type_name(device_type: iced::wgpu::DeviceType) -> &'static str {
    use iced::wgpu::DeviceType;
    match device_type {
        DeviceType::DiscreteGpu => "DiscreteGpu",
        DeviceType::IntegratedGpu => "IntegratedGpu",
        DeviceType::VirtualGpu => "VirtualGpu",
        DeviceType::Cpu => "Cpu",
        DeviceType::Other => "Other",
    }
}

/// 把 wgpu 的 future 跑到底。
///
/// 只用 `std`：`Waker` 由 `std::task::Wake` 提供（安全 API），运行期负责 `park`。
/// 不引入 futures 执行器——这里只有一个 future，而且它在 wgpu 的回调线程上唤醒。
#[cfg(feature = "gpu")]
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct Unpark(std::thread::Thread);

    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);

    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_line_is_greppable() {
        let failure = Failure {
            stage: "request-device",
            detail: "no device request succeeded".to_owned(),
        };
        let text = failure.describe();
        assert!(
            text.starts_with("FAILED stage=request-device error="),
            "{text}"
        );
        assert!(text.contains("no device request succeeded"), "{text}");
    }

    #[test]
    fn verdict_line_lists_the_three_facts() {
        let verdict = Verdict {
            adapter: "Apple M1".to_owned(),
            device_type: "IntegratedGpu",
            backend: "Metal".to_owned(),
        };
        let text = verdict.describe();
        assert!(text.contains("adapter=\"Apple M1\""), "{text}");
        assert!(text.contains("type=IntegratedGpu"), "{text}");
        assert!(text.contains("backend=Metal"), "{text}");
    }

    /// 没编 wgpu 的构建：预检必须说清原因，而不是一句「失败」。
    #[cfg(not(feature = "gpu"))]
    #[test]
    fn unbuilt_gpu_names_the_reason() {
        let failure = run().unwrap_err();
        assert_eq!(failure.stage, "not-compiled");
        assert!(failure.detail.contains("wgpu"), "{}", failure.detail);
    }
}
