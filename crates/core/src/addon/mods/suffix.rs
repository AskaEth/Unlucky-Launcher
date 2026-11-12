//! `.disabled` / `.old` 后缀的**唯一判据**（M224 · MODTOGGLE）。
//!
//! # 为什么这里是一份而不是三份
//!
//! 收敛前仓里有两份同构实现，各自 lowercase → 剥 `.old` → `ends_with(".disabled")`：
//!
//! | 位置 | 形状 | 用途 |
//! |---|---|---|
//! | `addon::export::scan::is_disabled_path` | 只给 bool | 导出扫描判禁用位 |
//! | `addon::import::plan::paths::suffix_variant` | 四态枚举 | 导入计划打注记 |
//!
//! 两份逐字同构，而 M218 的结论是「做启停时第一步先收敛成一份，别让第三份出生」
//! （`DevDocs/PlanDoc/08-mod-and-recommend.md` §3）。启停要**改写**文件名，比"判一下"多两件事：
//! ①必须保住原始大小写（`X.JAR.DISABLED` 禁用后不能变全小写）；②必须保住 `.old` 的位置
//! （`.disabled` 只能出现在 `.old` **之前**，否则 [`suffix_variant`] 认不出来）。
//!
//! 所以本模块是唯一出口：判据（[`suffix_variant`] / [`is_disabled_path`]）与改写目标名
//! （[`renamed_for`]）用**同一套剥离逻辑**，不会出现「判禁用用一套、改名字用另一套」。
//! 导出与导入两侧改为委派到本模块（函数名与签名保持不变）。
//!
//! # 认哪些形状（写死四种）
//!
//! | 磁盘上的名字 | 变体 | 禁用 | `.old` |
//! |---|---|---|---|
//! | `x.jar` | [`SuffixVariant::Plain`] | ❌ | ❌ |
//! | `x.jar.disabled` | [`SuffixVariant::Disabled`] | ✅ | ❌ |
//! | `x.jar.old` | [`SuffixVariant::Old`] | ❌ | ✅ |
//! | `x.jar.disabled.old` | [`SuffixVariant::DisabledOld`] | ✅ | ✅ |
//!
//! 大小写不敏感（Windows 上 `X.JAR.DISABLED` 与 `x.jar.disabled` 是同一个文件）；`.old` 只
//! 识别与保留（不清除、不改名——它是 HMCL 生态的旧版本残留，删掉等于替用户做决定）。

/// 禁用位的路径后缀（官方 Modrinth 导出的表达，也是唯一跨启动器保真的那条）。
pub const DISABLED_SUFFIX: &str = ".disabled";

/// 旧版本残留的路径后缀（只识别与保留）。
pub const OLD_SUFFIX: &str = ".old";

/// 路径后缀变体（`.disabled` / `.old` 的四种组合）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SuffixVariant {
    /// `mods/x.jar`。
    Plain,
    /// `mods/x.jar.disabled`（官方 Modrinth 的禁用表达）。
    Disabled,
    /// `mods/x.jar.old`（HMCL 生态常见：旧版本残留）。
    Old,
    /// `mods/x.jar.disabled.old`。
    DisabledOld,
}

impl SuffixVariant {
    /// 是否禁用态。
    #[must_use]
    pub const fn is_disabled(self) -> bool {
        matches!(self, Self::Disabled | Self::DisabledOld)
    }

    /// 是否带 `.old`（只用于报告，不附加清理语义）。
    #[must_use]
    pub const fn is_old(self) -> bool {
        matches!(self, Self::Old | Self::DisabledOld)
    }
}

/// 识别四种路径后缀变体。
#[must_use]
pub fn suffix_variant(path: &str) -> SuffixVariant {
    let lowered = path.trim_end().to_ascii_lowercase();
    let stripped = lowered.strip_suffix(OLD_SUFFIX).unwrap_or(&lowered);
    let old = stripped.len() != lowered.len();
    let disabled = stripped.ends_with(DISABLED_SUFFIX);
    match (disabled, old) {
        (false, false) => SuffixVariant::Plain,
        (true, false) => SuffixVariant::Disabled,
        (false, true) => SuffixVariant::Old,
        (true, true) => SuffixVariant::DisabledOld,
    }
}

/// 路径后缀是否带 `.disabled`（允许 `x.jar.disabled.old` 这种"禁用 + 旧残留"组合）。
#[must_use]
pub fn is_disabled_path(path: &str) -> bool {
    suffix_variant(path).is_disabled()
}

/// `enabled = true` 时该改成什么名字（`None` = 已经是启用态，不用动）。
///
/// `x.jar.disabled` → `x.jar`；`x.jar.disabled.old` → `x.jar.old`（`.old` 原位保留）。
/// 原始大小写按**逐字符比较**剥掉后缀，所以 `X.JAR.DISABLED` → `X.JAR`，不会被整串小写化。
#[must_use]
pub fn enabled_name(name: &str) -> Option<String> {
    let trimmed = name.trim_end();
    // **先剥 `.old` 再剥 `.disabled`**：反过来会漏 `x.jar.disabled.old`（结尾是 `.old`，
    // 直接找结尾的 `.disabled` 找不到 ⇒ 明明禁用着却说"已是启用态"）。这一条由
    // `toggling_round_trips_every_variant` / `enabling_only_strips_the_disabled_suffix` 钉住。
    let (body, old) = match strip_ascii_ci(trimmed, OLD_SUFFIX) {
        Some(base) => (base, Some(&trimmed[base.len()..])),
        None => (trimmed, None),
    };
    let stripped = strip_ascii_ci(body, DISABLED_SUFFIX)?;
    // 剥完只剩空串或只剩 `.old` 都不是文件名（`.disabled` / `.old.disabled` 这种畸形）。
    if stripped.is_empty() || stripped.eq_ignore_ascii_case(OLD_SUFFIX) {
        return None;
    }
    Some(match old {
        Some(old) => format!("{stripped}{old}"),
        None => stripped.to_string(),
    })
}

/// `enabled = false` 时该改成什么名字（`None` = 已经是禁用态，不用动）。
///
/// `x.jar` → `x.jar.disabled`；`x.jar.old` → `x.jar.disabled.old`——`.disabled` **插在** `.old`
/// 之前，否则 [`suffix_variant`] 读回来是 `Old`（"有旧残留但没禁用"），禁用位就丢了。
#[must_use]
pub fn disabled_name(name: &str) -> Option<String> {
    let trimmed = name.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    if suffix_variant(trimmed).is_disabled() {
        return None;
    }
    // 只认原串里那个 `.old`（大小写可能不同），按字节位置插进去，保住其余部分原样。
    if let Some(base) = strip_ascii_ci(trimmed, OLD_SUFFIX) {
        if base.is_empty() {
            return None;
        }
        return Some(format!("{base}{DISABLED_SUFFIX}{OLD_SUFFIX}"));
    }
    Some(format!("{trimmed}{DISABLED_SUFFIX}"))
}

/// 启停后应当落成的文件名（`None` = 已经是这个状态，无需 rename）。
#[must_use]
pub fn renamed_for(name: &str, enabled: bool) -> Option<String> {
    if enabled {
        enabled_name(name)
    } else {
        disabled_name(name)
    }
}

/// 展示名（去掉 `.disabled` / `.old` 两个后缀，其余原样）。
///
/// **不**做 `-版本号` 的猜解：那是 `diagnose::environment::ModRecord` 的活，这里再猜一次就是
/// 第二份启发式。列表要的是"这个文件叫什么"，不是"它可能是哪个 mod"。
#[must_use]
pub fn display_name(name: &str) -> &str {
    let trimmed = name.trim_end();
    // 与 [`enabled_name`] 同一个顺序：先 `.old`、再 `.disabled`（`x.jar.disabled.old` 两个都要剥）。
    let without_old = strip_ascii_ci(trimmed, OLD_SUFFIX).unwrap_or(trimmed);
    strip_ascii_ci(without_old, DISABLED_SUFFIX).unwrap_or(without_old)
}

/// 去掉**结尾**的 ASCII 后缀（大小写不敏感），保住前半段原始大小写。
fn strip_ascii_ci<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let cut = text.len().checked_sub(suffix.len())?;
    if !text.is_char_boundary(cut) {
        return None;
    }
    text.get(cut..)
        .filter(|tail| tail.eq_ignore_ascii_case(suffix))
        .map(|_| &text[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 四种形状都认得，且禁用位只看 `.disabled`（`.old` 单独出现不是禁用）。
    #[test]
    fn recognises_four_variants() {
        assert_eq!(suffix_variant("mods/x.jar"), SuffixVariant::Plain);
        assert_eq!(
            suffix_variant("mods/x.jar.disabled"),
            SuffixVariant::Disabled
        );
        assert_eq!(suffix_variant("mods/x.jar.old"), SuffixVariant::Old);
        assert_eq!(
            suffix_variant("mods/x.jar.disabled.old"),
            SuffixVariant::DisabledOld
        );
        assert!(is_disabled_path("mods/x.jar.disabled"));
        assert!(is_disabled_path("mods/x.jar.disabled.old"));
        assert!(!is_disabled_path("mods/x.jar.old"));
        assert!(!is_disabled_path("mods/x.jar"));
    }

    /// 大小写不敏感（Windows 上 `X.JAR.DISABLED` 就是禁用），尾随空白不算名字的一部分。
    #[test]
    fn suffix_matching_ignores_ascii_case() {
        assert_eq!(suffix_variant("X.JAR.DISABLED"), SuffixVariant::Disabled);
        assert_eq!(suffix_variant("X.Disabled.Old"), SuffixVariant::DisabledOld);
        assert_eq!(suffix_variant("x.jar.disabled \n"), SuffixVariant::Disabled);
        // 不是子串匹配：`.disabled` 必须真在结尾。
        assert_eq!(suffix_variant("x.disabled.jar"), SuffixVariant::Plain);
        assert_eq!(suffix_variant("x.jar.notdisabled"), SuffixVariant::Plain);
    }

    /// 启用 = 剥 `.disabled`，`.old` 原位保留，**原始大小写不动**。
    #[test]
    fn enabling_only_strips_the_disabled_suffix() {
        assert_eq!(enabled_name("x.jar.disabled").as_deref(), Some("x.jar"));
        assert_eq!(
            enabled_name("x.jar.disabled.old").as_deref(),
            Some("x.jar.old")
        );
        assert_eq!(enabled_name("X.JAR.DISABLED").as_deref(), Some("X.JAR"));
        // 已经是启用态 / 只剩畸形后缀 ⇒ None（不猜、不改）。
        assert_eq!(enabled_name("x.jar"), None);
        assert_eq!(enabled_name(".disabled"), None);
        assert_eq!(enabled_name(".old.disabled"), None);
    }

    /// 禁用 = 把 `.disabled` **插在** `.old` 之前（插在后面判据读不回来）。
    #[test]
    fn disabling_inserts_before_the_old_suffix() {
        assert_eq!(disabled_name("x.jar").as_deref(), Some("x.jar.disabled"));
        assert_eq!(
            disabled_name("x.jar.old").as_deref(),
            Some("x.jar.disabled.old")
        );
        assert_eq!(
            disabled_name("X.JAR.OLD").as_deref(),
            Some("X.JAR.disabled.old")
        );
        assert_eq!(disabled_name("x.jar.disabled"), None);
        assert_eq!(disabled_name("x.jar.disabled.old"), None);
        assert_eq!(disabled_name(""), None);
        assert_eq!(disabled_name(".old"), None);
    }

    /// 两个方向互为逆（同一条剥离逻辑的自证）：往返回来名字一模一样。
    #[test]
    fn toggling_round_trips_every_variant() {
        for plain in ["x.jar", "X.JAR", "风.jar", "a.b.c"] {
            let off = renamed_for(plain, false).expect("禁用目标名");
            assert_eq!(suffix_variant(&off), SuffixVariant::Disabled);
            assert_eq!(renamed_for(&off, true).as_deref(), Some(plain));
        }
        // `.old` 变体往返也不掉 `.old`。
        let off = renamed_for("x.jar.old", false).expect("禁用目标名");
        assert_eq!(off, "x.jar.disabled.old");
        assert_eq!(renamed_for(&off, true).as_deref(), Some("x.jar.old"));
        // 已经在目标状态 ⇒ None（幂等，不产生多余 rename）。
        assert_eq!(renamed_for("x.jar", true), None);
        assert_eq!(renamed_for("x.jar.disabled", false), None);
    }

    /// 展示名剥两个后缀，但**不**猜 `-版本号`。
    #[test]
    fn display_name_strips_suffixes_only() {
        assert_eq!(
            display_name("sodium-0.5.8.jar.disabled"),
            "sodium-0.5.8.jar"
        );
        assert_eq!(display_name("x.jar.disabled.old"), "x.jar");
        assert_eq!(display_name("jei.jar"), "jei.jar");
        assert_eq!(display_name("X.JAR.DISABLED"), "X.JAR");
    }
}
