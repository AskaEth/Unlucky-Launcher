//! 判据的**纯函数**部分：`请求 × 探测 × 是否编入 wgpu` → 起步后端 + 兜底后端。
//!
//! 表本身（什么条件选什么、日志该打什么、谁能验证）在 [`super`] 的模块文档顶部。
//! 这里只放可测的东西：判据一旦掺进运行期副作用（写环境变量、看日志、开窗），
//! 「降级矩阵」就再也写不成表驱动测试了——M97 的教训。

use crate::{Backend, Probe};

/// 一个具体渲染后端（`ICED_BACKEND` 的单项取值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `ICED_BACKEND=wgpu`。
    Wgpu,
    /// `ICED_BACKEND=tiny-skia`。
    TinySkia,
}

impl Kind {
    /// 写进 `ICED_BACKEND` 的取值（也是 `iced_renderer` 认的名字）。
    #[must_use]
    pub fn env_value(self) -> &'static str {
        match self {
            Kind::Wgpu => "wgpu",
            Kind::TinySkia => "tiny-skia",
        }
    }

    /// 日志里的名字。
    #[must_use]
    pub fn name(self) -> &'static str {
        self.env_value()
    }

    /// 对应的「档位」（D36 的 CPU 档 / GPU 档判据用）。
    #[must_use]
    pub fn tier(self) -> Backend {
        match self {
            Kind::Wgpu => Backend::Gpu,
            Kind::TinySkia => Backend::Cpu,
        }
    }
}

/// 判据产物：起步后端 + **本 crate 自己**的兜底（不是 iced 的候选回退）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    /// 写进 `ICED_BACKEND` 的唯一一项。
    pub primary: Kind,
    /// 起步失败后重开的后端；`None` = 没有兜底（强制 CPU / 没编 wgpu）。
    pub fallback: Option<Kind>,
    /// 为什么这么判（进日志，验收就靠它）。
    pub reason: &'static str,
}

impl Decision {
    /// 一行给人看的决策描述。
    #[must_use]
    pub fn describe(&self) -> String {
        match self.fallback {
            Some(fallback) => format!(
                "renderer = {} reason={} 兜底={}（失败会先打印具名错误）",
                self.primary.name(),
                self.reason,
                fallback.name()
            ),
            None => format!(
                "renderer = {} reason={} 兜底=none（候选表唯一项）",
                self.primary.name(),
                self.reason
            ),
        }
    }
}

/// 判据纯函数：请求 × 探测 × 是否编入 wgpu → 决策。
///
/// **必须是纯函数**：M97 的教训是判据一旦掺了运行期副作用（写环境变量、看日志），
/// 就再也写不出「降级矩阵」的表驱动测试了。
#[must_use]
pub fn decide(request: Backend, probe: &Probe, gpu_compiled: bool) -> Decision {
    match (request, gpu_compiled) {
        // 1. 用户点名 CPU：只有 CPU，且**不给兜底**（强制就是强制）。
        (Backend::Cpu, _) => Decision {
            primary: Kind::TinySkia,
            fallback: None,
            reason: "forced-cpu",
        },
        // 2. 点名 GPU 但没编 wgpu：只能 CPU，没得兜。
        (Backend::Gpu, false) => Decision {
            primary: Kind::TinySkia,
            fallback: None,
            reason: "gpu-not-compiled",
        },
        // 3. 点名 GPU 且编了：GPU 起步，CPU 兜底（绝不硬失败）。
        (Backend::Gpu, true) => Decision {
            primary: Kind::Wgpu,
            fallback: Some(Kind::TinySkia),
            reason: "forced-gpu",
        },
        // 4. auto + 没编 wgpu。
        (Backend::Auto, false) => Decision {
            primary: Kind::TinySkia,
            fallback: None,
            reason: "auto-gpu-not-compiled",
        },
        // 5. auto + 只有 WARP/llvmpipe 这类软光栅：CPU 起步，wgpu 留作最后兜底。
        (Backend::Auto, true) if !probe.hardware => Decision {
            primary: Kind::TinySkia,
            fallback: Some(Kind::Wgpu),
            reason: "auto-no-hardware-adapter",
        },
        // 6. auto + 有硬件适配器：GPU 起步，CPU 兜底。
        (Backend::Auto, true) => Decision {
            primary: Kind::Wgpu,
            fallback: Some(Kind::TinySkia),
            reason: "auto-hardware-adapter",
        },
    }
}

/// 预检失败时把 wgpu 降下来（判据表第 7 行）。
///
/// 只在 `ULL_PREFLIGHT=1` 且预检真的失败时调用：这时候**别去开那个窗**——重试一次要闪两下窗，
/// 而且原因已经拿在手里了。
#[must_use]
pub fn demote_after_preflight(decision: Decision) -> Decision {
    if decision.primary != Kind::Wgpu {
        return decision;
    }
    Decision {
        primary: Kind::TinySkia,
        fallback: Some(Kind::Wgpu),
        reason: "preflight-failed",
    }
}

/// 降级重开那一次的决策（判据表第 8 行的**子进程侧**，`boot_probe` 的第一分支用它）。
///
/// **`fallback` 必须是 `None`**：这一步的兜底就是「不再兜底」。若它非空，子进程再失败时
/// 还会 respawn ⇒ **无限递归重开**（每次重开都像「刚刚启动」，这种自杀式循环极难诊断）。
/// 所以这条不变量只在这里定义一次，并由 `decision::tests` 钉住，别在调用点另拼一个 `Decision`。
#[must_use]
pub fn fallback_round(kind: Kind) -> Decision {
    Decision {
        primary: kind,
        fallback: None,
        reason: "fallback-after-failure",
    }
}

/// 取证通道（`--screen=gallery`）用：**剥掉兜底**。
///
/// 硬规则（M97 定）：**取证工具宁可红着退出，也不能悄悄换一种后端继续出图**。
/// 以前 gallery 在 wgpu 挂掉时会静默出 CPU 截图，而报告上写的是「GPU 渲染截图」——
/// 所有视觉结论的可信度都被这一条悄悄打折，而且从输出上看不出来。
#[must_use]
pub fn strict(decision: Decision) -> Decision {
    Decision {
        fallback: None,
        reason: "gallery-strict",
        ..decision
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn probe(types: &[&'static str]) -> Probe {
        Probe {
            adapters: types
                .iter()
                .map(|device_type| crate::probe::AdapterSummary {
                    name: "adapter".to_owned(),
                    device_type,
                })
                .collect(),
            hardware: types
                .iter()
                .any(|device_type| !matches!(*device_type, "Cpu" | "Other")),
        }
    }

    /// 判据表的一行：`请求, 探测结论, 是否编入 wgpu, 期望起步, 期望兜底, 期望 reason`。
    type Case = (Backend, Probe, bool, Kind, Option<Kind>, &'static str);

    /// 判据表第 1~6 行逐行钉死（表驱动）。谁动了 `decide` 的矩阵，这条先红。
    #[test]
    fn criterion_table_rows_1_to_6() {
        let hardware = || probe(&["IntegratedGpu"]);
        let software = || probe(&["Cpu"]);
        let cases: &[Case] = &[
            // 1 强制 CPU：不给兜底
            (
                Backend::Cpu,
                hardware(),
                true,
                Kind::TinySkia,
                None,
                "forced-cpu",
            ),
            (
                Backend::Cpu,
                software(),
                false,
                Kind::TinySkia,
                None,
                "forced-cpu",
            ),
            // 2 点名 GPU 但没编
            (
                Backend::Gpu,
                hardware(),
                false,
                Kind::TinySkia,
                None,
                "gpu-not-compiled",
            ),
            // 3 点名 GPU 且编了
            (
                Backend::Gpu,
                hardware(),
                true,
                Kind::Wgpu,
                Some(Kind::TinySkia),
                "forced-gpu",
            ),
            (
                Backend::Gpu,
                software(),
                true,
                Kind::Wgpu,
                Some(Kind::TinySkia),
                "forced-gpu",
            ),
            // 4 auto + 没编
            (
                Backend::Auto,
                hardware(),
                false,
                Kind::TinySkia,
                None,
                "auto-gpu-not-compiled",
            ),
            // 5 auto + 只有软光栅
            (
                Backend::Auto,
                software(),
                true,
                Kind::TinySkia,
                Some(Kind::Wgpu),
                "auto-no-hardware-adapter",
            ),
            // 6 auto + 有硬件（macOS M1 走这条）
            (
                Backend::Auto,
                hardware(),
                true,
                Kind::Wgpu,
                Some(Kind::TinySkia),
                "auto-hardware-adapter",
            ),
        ];

        for (request, probe, gpu_compiled, primary, fallback, reason) in cases {
            let got = decide(*request, probe, *gpu_compiled);
            assert_eq!(got.primary, *primary, "request={request:?} reason={reason}");
            assert_eq!(
                got.fallback, *fallback,
                "request={request:?} reason={reason}"
            );
            assert_eq!(got.reason, *reason);
        }
    }

    /// **安全网守门**：除了「强制 CPU」和「没编 wgpu」，任何一条判据都必须留兜底。
    ///
    /// 抓不住：兜底那次重开本身失败（那是 iced 的事，见判据表第 8 行）。
    #[test]
    fn every_decision_keeps_an_exit() {
        for request in [Backend::Auto, Backend::Gpu, Backend::Cpu] {
            for gpu_compiled in [true, false] {
                for types in [&[][..], &["Cpu"][..], &["IntegratedGpu"][..]] {
                    let decision = decide(request, &probe(types), gpu_compiled);
                    let forced = request == Backend::Cpu || !gpu_compiled;
                    assert_eq!(
                        decision.fallback.is_none(),
                        forced,
                        "{decision:?} 必须留兜底（request={request:?} gpu_compiled={gpu_compiled}）"
                    );
                    // 兜底不能和起步项相同，否则重试等于原地打转。
                    assert_ne!(decision.fallback, Some(decision.primary), "{decision:?}");
                }
            }
        }
    }

    /// 预检失败只降 wgpu 起步的那些行；`--renderer=cpu` 一行都不许动。
    #[test]
    fn preflight_demotes_only_wgpu_decisions() {
        let hardware = probe(&["IntegratedGpu"]);
        let auto = decide(Backend::Auto, &hardware, true);
        let demoted = demote_after_preflight(auto);
        assert_eq!(demoted.primary, Kind::TinySkia);
        assert_eq!(demoted.reason, "preflight-failed");
        assert_eq!(demoted.fallback, Some(Kind::Wgpu), "wgpu 还能当最后兜底");

        let cpu = decide(Backend::Cpu, &hardware, true);
        assert_eq!(demote_after_preflight(cpu), cpu, "强制 CPU 不许被预检改写");
    }

    /// 取证通道的决策必须**剥掉兜底**（判据表第 9 行）：`strict` 若忘了清 `fallback`，
    /// gallery 就能在 wgpu 挂掉时悄悄出 CPU 图——那是 M97 要杀的第九种「假绿」。
    #[test]
    fn strict_strips_the_fallback() {
        let gpu = decide(Backend::Gpu, &Probe::default(), true);
        assert_eq!(gpu.fallback, Some(Kind::TinySkia), "前提：普通决策有兜底");
        let strict = strict(gpu);
        assert_eq!(strict.fallback, None, "取证通道不许有兜底");
        assert_eq!(strict.primary, Kind::Wgpu, "剥的是兜底，不是起步项");
        assert_eq!(strict.reason, "gallery-strict");
    }

    #[test]
    fn describe_names_the_backend_and_the_reason() {
        let hardware = probe(&["DiscreteGpu"]);
        let text = decide(Backend::Auto, &hardware, true).describe();
        assert!(text.contains("renderer = wgpu"), "{text}");
        assert!(text.contains("reason=auto-hardware-adapter"), "{text}");
        assert!(text.contains("兜底=tiny-skia"), "{text}");
    }
}
