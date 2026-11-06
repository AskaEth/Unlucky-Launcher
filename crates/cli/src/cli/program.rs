//! 程序名：CLI 输出里自称什么，取决于**被调用的那个二进制**叫什么。
//!
//! GUI 二进制（`unlucky-launcher`）与 `ull` 共用同一套命令行（M82），所以 banner / 用法行 /
//! 「`<名字> --help`」这类提示不能写死 `ull` —— 用户敲的是 `unlucky-launcher`，把他指去敲
//! 一个可能不存在的名字只会让人困惑。
//!
//! 做法（M82 r2 定案）：文案里放**占位符**，渲染时替换；**绝不对插值后的整条文本做词替换**。
//! 上一版按「独立词 `ull`」替换整条消息，于是用户数据被改坏过：账户名叫 `ull` 的会变成
//! `unlucky-launcher`，路径 `D:/tmp/ull/.ULL` 会变成 `D:/tmp/unlucky-launcher/.ULL`。
//! 占位符只在模板里出现，插值进来的用户名/路径碰不到它。
//!
//! 取不到 argv[0]（某些 exec 场景）就回落 [`FALLBACK`]，输出因此与老版本逐字相同。

/// 文案里的程序名占位符。
///
/// 用 `%prog%` 而不是 `{prog}`：这些模板要活在 `format!` 里，`{prog}` 会被 `format!` 当成
/// 参数名直接编译失败，而 `%prog%` 原样穿过去，到渲染时才替换。
pub const PLACEHOLDER: &str = "%prog%";

/// 取不到 argv[0] 时的回落名（也是 `crates/cli` 产出的那个 bin）。
pub const FALLBACK: &str = "ull";

/// 从 argv[0] 求程序名：剥目录、剥 `.exe`、空值回落。
///
/// Windows 上是 `C:\…\unlucky-launcher.exe`，unix 上是 `./target/release/unlucky-launcher`，
/// 两种都要得到 `unlucky-launcher`。
#[must_use]
pub fn name_in(argv0: Option<&str>) -> String {
    let Some(raw) = argv0 else {
        return FALLBACK.to_owned();
    };
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let base = strip_exe(base);
    if base.is_empty() {
        FALLBACK.to_owned()
    } else {
        base.to_owned()
    }
}

fn strip_exe(name: &str) -> &str {
    match name
        .strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".EXE"))
    {
        Some(stem) => stem,
        None => name,
    }
}

/// 当前进程的程序名。
#[must_use]
pub fn name() -> String {
    let argv0 = std::env::args_os()
        .next()
        .map(|arg| arg.to_string_lossy().into_owned());
    name_in(argv0.as_deref())
}

/// 把模板里的 [`PLACEHOLDER`] 换成程序名。
///
/// **只认占位符**：文本里其它位置的 `ull`（账户名、路径段、回显的子命令）原样保留——这是
/// 与「按词替换整条文本」的分界线，也是本函数唯一要保证的事。
#[must_use]
pub fn fill(text: &str) -> String {
    if !text.contains(PLACEHOLDER) {
        return text.to_owned();
    }
    text.replace(PLACEHOLDER, &name())
}

#[cfg(test)]
mod tests {
    use super::{fill, name_in, FALLBACK, PLACEHOLDER};

    #[test]
    fn argv0_is_reduced_to_a_bare_name() {
        assert_eq!(
            name_in(Some(r"C:\Program Files\ULL\unlucky-launcher.exe")),
            "unlucky-launcher"
        );
        assert_eq!(
            name_in(Some("./target/release/unlucky-launcher")),
            "unlucky-launcher"
        );
        assert_eq!(name_in(Some("ull")), "ull");
        assert_eq!(name_in(Some("/usr/local/bin/ull.exe")), "ull");
    }

    #[test]
    fn empty_or_missing_argv0_falls_back() {
        assert_eq!(name_in(None), FALLBACK);
        assert_eq!(name_in(Some("")), FALLBACK);
        assert_eq!(name_in(Some("/usr/bin/")), FALLBACK);
    }

    #[test]
    fn fill_replaces_only_the_placeholder() {
        assert_eq!(
            fill(&format!("{PLACEHOLDER} --help 查看命令列表")),
            format!("{} --help 查看命令列表", super::name())
        );
        // 没有占位符就原样返回（不碰任何词）。
        assert_eq!(fill("ull --help"), "ull --help");
    }

    #[test]
    fn user_data_that_says_ull_is_never_rewritten() {
        // 回归：账户名恰好是 `ull` / 路径里有 `\ull\` / 回显的子命令是 `ull`，
        // 一个字节都不能被改成程序名（M82 r1 的真缺陷）。
        for text in [
            "找不到账户 `ull`",
            r"索引文件：D:/tmp/ull\.ULL\data\agent\sessions\index.jsonl",
            "未知命令 `ull`",
            "过滤词 null 与 pull 也不算",
        ] {
            assert_eq!(fill(text), text, "{text} 被改了");
        }
    }
}
