//! 不定态的**节奏**：把官方的 keyframes 摊成纯函数 `phase → 进度`。
//!
//! ## 为什么组件不计时
//!
//! 契约 §2：组件不持有时钟/状态所有权。`indeterminate` 的相位由**调用方**给（`phase` 0..1，
//! 由页面的 `Subscription` 每帧/每 tick 推进）。好处不只「符合契约」：截图 harness 也才能拍到
//! 确定的中间帧——否则每张图都是一次抓阄。
//!
//! ## 数据来源（A1，`javap` 读本机 AAR 得到，不是凭记忆）
//!
//! ```text
//! ~/.gradle/caches/.../material3-android/1.4.0/.../material3.aar → classes.jar
//! javap -p -c -cp classes.jar androidx.compose.material3.ProgressIndicatorKt
//!   ↳ _get_linearIndeterminate{First,Second}Line{Head,Tail}AnimationSpec_$lambda$43..46
//! ```
//!
//! Compose 的 `KeyframesSpec.at(value, time).using(easing)` 是「**从该点出发的那一段**用这条缓动」，
//! 所以 [`KeyPoint::easing`] 的语义就是段起点的缓动；最后一点之后一律**保持末值**
//! （Compose 对未覆盖的区间就是保持，这也是「段跑到头以后停在满格」的来源）。
//!
//! ## 首点的两种合法情况（复审 P1-B 的教训）
//!
//! Compose 在 keyframes **缺 0ms 关键帧**时会自己补一个 `t = 0`、值 = 动画**初值**的点
//! （`VectorizedKeyframesSpec` 构造里 `Map.containsKey(0) == false` 就 `timestamps.add(0, 0)`，
//! 且 `playTime <= 0 → startValue`）。本求值器对这种情况按「保持首值」处理，而它**只等价于**
//! Compose 的「初值」—— 所以守卫是「首点不在 0ms **且** 首值 ≠ 0」才 `debug_assert` 炸：
//!
//! - `SEG_TAIL_1/2`（首点 250/900ms、首值 **0**）与 Compose 的「初值 0」等价，**合法**；
//! - `ROTATION` 曾经首点 300ms/**0.25**：`[0, 0.05)` 被恒定成 90°、周期末尾 360° → 下周期
//!   90° 瞬时倒跳 270°。现在首点显式补成 0ms/0°。

use unlucky_design::motion::Easing;

/// 一个关键帧：`ms` 时刻的值 + **从它出发那一段**的缓动。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyPoint {
    /// 相对周期起点的毫秒数。
    pub ms: u32,
    /// 该时刻的进度（0..1，环形是「弧长占比」，旋转是「度数 / 360」）。
    pub value: f32,
    /// 从本点出发到下一个点的那一段使用的缓动。
    pub easing: Easing,
}

/// 一条 keyframes 曲线（一个周期内可采样任意相位）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframes {
    /// 周期长度（毫秒）。`phase` 按它折算成时间，再对周期取模（相位可越界/可循环）。
    pub cycle_ms: u32,
    /// 关键帧（按 `ms` 升序）。
    pub points: &'static [KeyPoint],
}

impl Keyframes {
    /// 采样：`phase`（任意实数，按周期取模）→ 进度。
    ///
    /// 空列表给 0；`cycle_ms == 0` 时直接给首点的值（没有可循环的时间轴）。
    pub fn sample(&self, phase: f32) -> f32 {
        let Some(first) = self.points.first() else {
            return 0.0;
        };
        if self.cycle_ms == 0 || !phase.is_finite() {
            return first.value;
        }

        let cycle = self.cycle_ms as f32;
        // 首点不在 0ms 时，`[0, first.ms)` 按「保持首值」算；Compose 那里放的是动画**初值**。
        // 两者只在「首值 = 初值 = 0」时等价 —— 不等价就会画出一段不存在的静止（ROTATION 曾因此
        // 每周期末尾倒跳 270°）。所以这里只拦「首点不在 0 且首值不是 0」。
        debug_assert!(
            first.ms == 0 || first.value == 0.0,
            "首点 {}ms 的值是 {}：Compose 在 [0, {}ms) 用的是动画初值 0，不是首值",
            first.ms,
            first.value,
            first.ms
        );
        let ms = phase.rem_euclid(1.0) * cycle;
        if ms <= first.ms as f32 {
            return first.value;
        }

        for pair in self.points.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            if ms > to.ms as f32 {
                continue;
            }
            let span = to.ms as f32 - from.ms as f32;
            if span <= 0.0 {
                return to.value;
            }
            let t = (ms - from.ms as f32) / span;
            return from.value + (to.value - from.value) * from.easing.sample(t);
        }

        self.points[self.points.len() - 1].value
    }
}
