//! M217 守卫：**整棵 `crates/` 树**里所有子进程都必须经 `platform::child::quiet` 起。
//!
//! M213 建它时射程只有 `crates/core/src`；用户随后报「CI 构建版启动时底下闪一个控制台」，
//! 定位到 `app-iced/src/platform/locale.rs` 的 `reg.exe`——射程之外的漏网。M217 把根抬到
//! `crates/`，角色路径也重写成相对 `crates/`。
//!
//! ## 判据（按角色，不按次数）
//!
//! 每一处直接构造的**文件**必须命中 [`ALLOWED_ROLES`] 里的某条角色，否则就是绕开了构造器；红的
//! 信息是「某文件某行不属于任何角色」而不是「总数对不上 N」，所以它咬得住「随手再 spawn 一个
//! 命令」。文件角色还钉**精确处数**；目录角色（`core/tests/` 这类）只要求至少一处，因为测试
//! 夹具的数量是活的，钉死会变成假红。
//!
//! ## 抓不住什么（别当完备保证）
//!
//! - **别名**：只认 `Command as <别名>` 与 `type <别名> = …Command`；两跳别名、从别的模块
//!   `pub use` 进来的别名看不见。`Command` 没有 `From`，非 `::new` 的构造暂不存在。
//! - **注释与空白**：只剥整行 `//` 与**行首** `/*`；行尾注释里写 `Command::new`、行中开始的
//!   跨行块注释之后那几行会**误报**（方向「宁可红」，与 P1-1 的假绿相反）。判据是字面子串，
//!   `Command :: new(..)` 看不见（`rustfmt` 会规范回去）。
//! - **射程**：根是 `crates/`；`packaging/*.py`、`.github/workflows/*` 里的 shell 不在内（父进程
//!   是控制台，GUI 下不闪窗），`crates/app`（Slint 退役树，非 workspace 成员）**在**内。
//! - **行为**：只证明「代码里没有绕开构造器」，不证明「桌面上真的没闪窗口」——本机无前台桌面，
//!   做不到目视验证（见 M217 报告）。
//!
//! ## 变异验证（全部实测过并已还原，只改实现侧）
//!
//! M213（射程还是 `crates/core/src`）：①`download/region_system.rs` 塞裸 `tzutil` ⇒ 红并点名该
//! 行；②同一处改写成别名 `Spawn::new(..)` ⇒ 同样红；③删 `platform/child.rs` 的
//! `creation_flags(CREATE_NO_WINDOW)` ⇒ [`the_constructor_actually_passes_create_no_window`] 红；
//! ④`game/java/discover.rs` 的 `quiet("reg")` 改回裸构造 ⇒ 必须红（见 P1-1）。
//!
//! M217（落在**此前不在射程内**的 crate 上）：⑤`presenter/src/host/env.rs` 塞裸
//! `Command::new("tzutil")` ⇒ 红并点名 `crates/presenter/src/host/env.rs:<行>`；⑥
//! `app-iced/src/platform/locale.rs` 的 `quiet(program)` 改回裸构造 ⇒ 红（用户报的那处
//! `reg.exe`）；⑦`export.rs` 三个 opener 留裸 + 清单写明理由 ⇒ **不**红。
//!
//! ## P1-1：守卫自己被变异验了两遍才对的
//!
//! ③第一次**没红**：断言搜整份源码，而 `quiet` 的文档注释里就写着
//! `.creation_flags(CREATE_NO_WINDOW)`——注释喂饱了断言，改成先剥注释（[`code_lines`]）再判。
//! ④（复审判的变异 D）**又没红**：那版 `code_lines` 对每行找 `/*`，而注释与字符串里到处是
//! glob（`/usr/lib/jvm/*`、`"mods/*.jar"`），这些 `/*` 没有配对的 `*/` ⇒ `in_block` 一旦为真就
//! 不再变假，**该文件从那一行到 EOF 全被吞掉**（`crates/core/src` 里 8/371 个中招，一个正是
//! `game/java/discover.rs`）。现在只在**行首**认 `/*`，并由
//! [`the_scan_reaches_the_end_of_every_file`] 逐文件核对最后一行代码必须被扫到。

use std::fs;
use std::path::{Path, PathBuf};

/// 允许直接构造子进程的一条**角色**。
struct Role {
    /// 相对 `crates/` 的路径。以 `/` 结尾 = 该目录下全部文件（目录角色）。
    path: &'static str,
    /// 期望的裸构造处数：`Some(n)` 精确核对；目录角色 `None` = 只要求至少一处。
    sites: Option<usize>,
    /// 为什么这一处不能走 `quiet`。文件角色写这一处，目录角色写这一整类。
    why: &'static str,
}

impl Role {
    /// 目录角色覆盖该目录下全部文件；文件角色按整条路径精确比。
    fn covers(&self, rel: &str) -> bool {
        (self.path.ends_with('/') && rel.starts_with(self.path)) || rel == self.path
    }
}

/// 这张表是**角色**清单，不是「白名单文件」清单：新增一行必须写清「为什么不能走 `quiet`」。
const ALLOWED_ROLES: &[Role] = &[
    Role {
        path: "core/src/platform/child.rs",
        sites: Some(1),
        why: "构造器自身：`child::quiet` 内部就是这一处 `Command::new`",
    },
    Role {
        path: "core/src/launch/process.rs",
        sites: Some(1),
        why: "游戏进程：要 CREATE_NEW_PROCESS_GROUP 撑 taskkill /T 回收进程树；\
              游戏要不要带自己的控制台窗口是产品决定，不是顺手抑制（M213 明确不动）",
    },
    Role {
        path: "app-iced/src/app/dialog/export.rs",
        sites: Some(3),
        why: "目录打开器三兄弟（explorer / open / xdg-open，各平台一个）：用户点「打开文件夹」\
              要的就是那个文件管理器窗口，窗口就是产品。这三者自己都是 GUI 侧程序，由无控制台的\
              GUI 父进程 spawn 时本来就不分控制台；改错的代价是用户点按钮没反应（M217 要求留裸）",
    },
    Role {
        path: "presenter/src/host/opener.rs",
        sites: Some(1),
        why: "浏览器打开器（rundll32 / open / xdg-open）：与上一行同一角色——打开一个可见的东西\
              是这次 spawn 的**目的**。M217 不改它（不在本 mission 的改动范围），只登记成角色，\
              免得守卫把正确的实现判成漏网",
    },
    Role {
        path: "core/tests/",
        sites: None,
        why: "测试夹具：父进程是控制台（`cargo test`），子进程继承同一块控制台，不会新建窗口；\
              而且夹具要起 shell / tasklist / kill 之类的真程序做行为验证",
    },
    Role {
        path: "cli/tests/",
        sites: None,
        why: "测试夹具（同上；这里还要起真二进制验 CLI 的退出码与 update 签名）",
    },
];

#[test]
fn no_spawn_site_bypasses_the_quiet_constructor() {
    let root = crates_root();
    let files = rust_files(&root);
    // 兜底一：扫不到任何文件 ⇒ 根路径错了。兜底二：根扫到「非空但不是 crates/」也红——拿角色
    // 文件当锚点。
    assert!(
        !files.is_empty(),
        "没扫到任何 .rs：`crates/` 路径不对（{}）",
        root.display()
    );
    for anchor in ["core/src/platform/child.rs", "core/src/launch/process.rs"] {
        assert!(
            files.iter().any(|rel| rel == anchor),
            "扫描根不是 `crates/`：里面找不到 `{anchor}`（实际根：{}）",
            root.display()
        );
    }

    // 一次扫面：`(相对 crates/ 的路径, 行号, 该行代码)`。
    let mut sites: Vec<(String, usize, String)> = Vec::new();
    for rel in &files {
        let text = read_source(&root, rel);
        let aliases = command_aliases(&text);
        for (number, line) in code_lines(&text) {
            if is_spawn_site(line, &aliases) {
                sites.push((rel.clone(), number, line.trim().to_string()));
            }
        }
    }
    let illegal: Vec<String> = sites
        .iter()
        .filter(|(rel, ..)| !ALLOWED_ROLES.iter().any(|role| role.covers(rel)))
        .map(|(rel, number, line)| format!("  crates/{rel}:{number}  {line}"))
        .collect();
    assert!(
        illegal.is_empty(),
        "发现绕开 `platform::child::quiet` 的子进程构造（{} 处）：\n{}\n\
         在 Windows 上这会为每个子进程新建一块控制台（GUI 父进程下就是桌面上「黑框闪一下」）。\
         改走 `unlucky_core::platform::child::quiet(program)`；若这一处确实不该抑制窗口，\
         把它的**角色**加进 ALLOWED_ROLES 并写清理由。",
        illegal.len(),
        illegal.join("\n"),
    );

    // 角色核对：允许的角色必须真的存在、处数对得上，否则这张表会悄悄变成摆设。
    for role in ALLOWED_ROLES {
        let hits: Vec<usize> = sites
            .iter()
            .filter(|(rel, ..)| role.covers(rel))
            .map(|(.., number, _)| *number)
            .collect();
        match role.sites {
            Some(expected) => assert_eq!(
                hits.len(),
                expected,
                "角色 `crates/{}`（{}）应当恰好 {expected} 处直接构造，实测行号 {hits:?}",
                role.path,
                role.why,
            ),
            None => assert!(
                !hits.is_empty(),
                "目录角色 `crates/{}`（{}）一处都没命中——这条清单条目该删了",
                role.path,
                role.why,
            ),
        }
    }
}

#[test]
fn the_constructor_actually_passes_create_no_window() {
    let text = read_source(&crates_root(), "core/src/platform/child.rs");
    // **只看代码行**：`quiet` 的文档注释里就写着 `.creation_flags(CREATE_NO_WINDOW)`，
    // 对着整份源码搜会被那句注释喂饱——删掉真正的设置行也照样绿（实测踩过）。
    let code: Vec<&str> = code_lines(&text)
        .into_iter()
        .map(|(_, line)| line)
        .collect();
    // 只去空白，避免格式调整造成假红（下划线是标识符的一部分，不能去）。
    let no_space = code.concat().split_whitespace().collect::<String>();
    assert!(
        no_space.contains("creation_flags(CREATE_NO_WINDOW)"),
        "`platform::child::quiet` 不再设置 creation_flags —— 抑制窗口的那一行被删了？\n\
         注意 `.stdout(Stdio::null())` 不能替代它：那只改句柄，控制台窗口照样会被创建。",
    );
    // 常量值单独比：数字里的下划线是排版自由，去掉再比。
    let definition = code
        .iter()
        .find(|line| line.trim_start().starts_with("const CREATE_NO_WINDOW"))
        .expect("找不到 `CREATE_NO_WINDOW` 的常量定义");
    let (_, value) = definition.split_once('=').expect("常量定义里没有 `=`");
    let digits = value.replace('_', "");
    let literal = digits.split_whitespace().collect::<String>();
    assert_eq!(
        literal.trim_end_matches(';'),
        "0x08000000",
        "`CREATE_NO_WINDOW` 的取值不对（`{definition}`）。必须是 0x0800_0000，\
         不是 CREATE_NEW_CONSOLE（0x0000_0010，意思正好相反）。",
    );
}

#[test]
fn code_lines_is_not_fooled_by_glob_slashes_in_strings_or_docs() {
    // M213 复审 P1-1 的最小复现：**字符串**里的 `/*` 不许把后一行吞掉。
    let in_string = code_lines("let a = \"mods/*.jar\";\nlet b = 1;\n");
    assert!(
        in_string
            .iter()
            .any(|(number, line)| *number == 2 && line.contains("let b")),
        "字符串里的 `/*` 吞掉了后一行：{in_string:?}",
    );

    // 同一份故障在**文档注释的 glob** 上的形态（`crates/core/src` 里 8 个文件都是这一种）。
    let in_doc_glob = code_lines("//! | Linux | `/usr/lib/jvm/*` |\nfn real_code() {}\n");
    assert_eq!(
        in_doc_glob.len(),
        1,
        "注释里的 glob 吞掉了代码：{in_doc_glob:?}"
    );
    assert!(in_doc_glob[0].1.contains("fn real_code"));

    // 真的块注释还是要被剥掉（别把上面两条修成「块注释一律不认」）。
    let block = code_lines("/* a\n b\n*/\nfn after() {}\n");
    assert_eq!(block.len(), 1, "块注释没被剥：{block:?}");
    assert!(block[0].1.contains("fn after"), "剥错了行：{block:?}");

    // 同一行闭合的块注释，收尾后的残段是代码。
    let inline_close = code_lines("/* c */ fn inline() {}\n");
    assert_eq!(
        inline_close.len(),
        1,
        "同行闭合的块注释丢了残段：{inline_close:?}"
    );
    assert!(inline_close[0].1.contains("fn inline"));
}

#[test]
fn the_scan_reaches_the_end_of_every_file() {
    // 「整份文件被吞掉」这类故障自己红：逐文件核对**最后一行代码的行号**必须出现在扫面结果里。
    // 这条比「数总行数」尖锐——文件普遍以 `}` 结尾，光比内容会被别处的 `}` 蒙混过去。
    // 只有 `//!` 的结论页（`app-iced/src/platform/linux/mod.rs` 这类）没有代码行，跳过是对的。
    let root = crates_root();
    let files = rust_files(&root);
    assert!(!files.is_empty(), "没扫到任何 .rs：{}", root.display());

    let (mut visible_total, mut line_total) = (0usize, 0usize);
    let mut starved: Vec<String> = Vec::new();
    for rel in &files {
        let text = read_source(&root, rel);
        let visible = code_lines(&text);
        visible_total += visible.len();
        line_total += text.lines().count();
        let mut last_code = None;
        for (index, line) in text.lines().enumerate() {
            if looks_like_code(line) {
                last_code = Some((index + 1, line));
            }
        }
        let Some((number, line)) = last_code else {
            continue;
        };
        if !visible.iter().any(|(seen, _)| *seen == number) {
            starved.push(format!(
                "crates/{rel}：最后一行代码 `{}`（第 {number} 行）没被扫到 ⇒ 被吞到 EOF",
                line.trim()
            ));
        }
    }
    assert!(starved.is_empty(), "扫面没走到底：\n{}", starved.join("\n"));
    // 兜底：可见行占比实测 81%（1219 个 .rs），门槛取 60% 留足余量。它只抓「大面积吞行」。
    assert!(
        visible_total * 10 >= line_total * 6,
        "可见行只有 {visible_total}/{line_total}（<60%），扫面异常缩水",
    );
}

/// 「这一行看起来是代码」：非空，且不是以 `/`（`//`、`/*`）或 `*`（`*/`、续行）开头。
fn looks_like_code(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && !trimmed.starts_with(['/', '*'])
}

/// `crates/`（`CARGO_MANIFEST_DIR` 的上一级）。守卫的射程就是它，不多不少。
fn crates_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = manifest.parent().expect("`crates/core` 没有上一级");
    crates.to_path_buf()
}

fn read_source(root: &Path, rel: &str) -> String {
    fs::read_to_string(root.join(rel))
        .unwrap_or_else(|error| panic!("读不到 crates/{rel}：{error}"))
}

fn rust_files(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let entries =
            fs::read_dir(dir).unwrap_or_else(|error| panic!("读不到 {}：{error}", dir.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let rel = path.strip_prefix(root).expect("strip_prefix 失败");
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    let mut files = Vec::new();
    walk(root, root, &mut files);
    files.sort();
    files
}

/// 去掉注释后的可见行（行号保留，报错能指到原文件）。
///
/// **`/*` 只在行首才算块注释开头**（M213 复审 P1-1）：本仓库的文档注释与字符串里到处是 glob
/// （`/usr/lib/jvm/*`、`"mods/*.jar"`），行中认 `/*` 会让 `in_block` 一去不回，把该文件从那
/// 里到 EOF 全吞掉。代价是行中开始的跨行块注释之后那几行会被当成代码（方向：误报）。
fn code_lines(text: &str) -> Vec<(usize, &str)> {
    let mut visible: Vec<(usize, &str)> = Vec::new();
    let mut in_block = false;
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let trimmed = raw.trim_start();
        if in_block {
            // 块内只认 `*/`：它后面剩的残段可能是代码（`*/ let x = 1;`）。
            if let Some(end) = trimmed.find("*/") {
                in_block = false;
                push_tail(&mut visible, number, &trimmed[end + 2..]);
            }
            continue;
        }
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with("/*") {
            match trimmed.find("*/") {
                Some(end) => push_tail(&mut visible, number, &trimmed[end + 2..]),
                None => in_block = true,
            }
            continue;
        }
        visible.push((number, raw));
    }
    visible
}

/// 块注释收尾后的残段：不是空、也不是新的一条整行注释，才算可见代码。
fn push_tail<'a>(visible: &mut Vec<(usize, &'a str)>, number: usize, tail: &'a str) {
    let tail = tail.trim_start();
    if !tail.is_empty() && !tail.starts_with("//") {
        visible.push((number, tail));
    }
}

/// 本文件里由 `std::process::Command` 导出的别名（`use ... Command as X;` / `type X = …Command;`）。
fn command_aliases(text: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    for (_, line) in code_lines(text) {
        // `use std::process::Command as Spawn;` / `use std::process::{Command as Spawn};`
        if let Some(at) = line.find("Command as ") {
            let ident: String = line[at + "Command as ".len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !ident.is_empty() {
                aliases.push(ident);
            }
            continue;
        }
        // `type Cmd = std::process::Command;` —— 别名在 `=` 的**左边**。
        let trimmed = line.trim_start();
        if trimmed.starts_with("type ") && line.ends_with("Command;") {
            if let Some(ident) = trimmed
                .split_whitespace()
                .find(|word| *word != "type" && !word.contains('='))
            {
                aliases.push(ident.to_string());
            }
        }
    }
    aliases
}

fn is_spawn_site(line: &str, aliases: &[String]) -> bool {
    line.contains("Command::new(")
        || aliases
            .iter()
            .any(|alias| line.contains(&format!("{alias}::new(")))
}
