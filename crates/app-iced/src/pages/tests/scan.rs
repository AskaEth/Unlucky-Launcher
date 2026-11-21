//! **目录派生**（M338）：把「谁参加判据」从手写清单换成扫源码树。
//!
//! ## 为什么要有这个文件
//!
//! 三处手写清单（页面源 / 接线期望 / 页面 key）长期挂着同一条技术债：**加文件或搬家时它们不自己红**。
//! 新增一个 `pages/**` 下的 `.rs` 而忘了往表里登记，守卫就静默失效——它只遍历表里那几个。
//! 所以这里把「参加者的来源」换成 `read_dir` 派生：**断言的内容一个字没改，改的只是谁参加**。
//!
//! ## 三条纪律（每条都对着本仓栽过的形态）
//!
//! 1. **读不到不许静默返回空**：目录读不动 / 文件读不动 / 一个文件都没扫到，一律 panic。
//!    空集合上的全称断言是恒真的——「路径拼错」会变成「全绿但什么都没守」（塔的 M338 提醒）。
//! 2. **空集保护在调用方**：每张表自己钉一条下界（扫到的文件数 ≥ 当前已知数），
//!    防「有人清空目录后测试依然全绿」。数字是各表自己数的，不互相抄。
//! 3. **用例文件不参加产品判据**：`*/tests.rs`、`*/tests/**`、`*_tests.rs`、`tests_*.rs` 是样本与夹具，
//!    它们里面写接线 / 写字面量是**应该的**。
//!
//! ## 与 `include_str!` 的关系
//!
//! 原来靠 `include_str!`（编译期）：编译不过 = 文件没了，那是它的好处。运行期读源码树只有在
//! **路径与 CI checkout 一致**时才等价，所以第 1、2 条不是形式主义，是这个改法的安全网。
//! 手法与 `components/gate.rs` / `pages/tests/rail_width.rs` 的扫描器同一套（`CARGO_MANIFEST_DIR` +
//! 递归 `read_dir`），本仓已有先例就不再造第二种写法。

use std::path::{Path, PathBuf};

/// 一个页面源文件：`name` 相对 `pages/`（[`source_at`] 的来自 `src/`），`text` 是全文。
pub(crate) struct Source {
    pub(crate) name: String,
    pub(crate) text: String,
}

fn src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn pages_dir() -> PathBuf {
    src_dir().join("pages")
}

/// `pages/<under>` 下所有**非测试** `.rs`（`under` 为空 = `pages/` 本身），按名字排序。
///
/// # Panics
/// 目录不存在 / 读不动 / 一个文件都没扫到 ⇒ panic（见模块文档第 1 条）。
pub(crate) fn sources_under(under: &str) -> Vec<Source> {
    let dir = if under.is_empty() {
        pages_dir()
    } else {
        pages_dir().join(under)
    };

    let mut files = Vec::new();
    rust_files(&dir, &mut files);

    let sources = collect(files, &pages_dir());
    assert!(
        !sources.is_empty(),
        "{} 下一个非测试 .rs 都没扫到：目录派生失效了（路径拼错？）",
        dir.display()
    );
    sources
}

/// 一个页面**族**的源码：`pages/<name>.rs`（页根）+ `pages/<name>/**`，两者都从磁盘发现。
///
/// 为什么不直接 `sources_under(<name>)`：页根文件在族目录**外面**（`pages/download.rs` 与
/// `pages/download/` 并列），只扫目录会漏掉它；按族取则「新增一个族内的文件」自动参加。
///
/// # Panics
/// 页根与族目录都不存在 ⇒ panic。
pub(crate) fn family(name: &str) -> Vec<Source> {
    let root = pages_dir().join(format!("{name}.rs"));
    let dir = pages_dir().join(name);

    let mut files = Vec::new();
    if root.is_file() {
        files.push(root);
    }
    if dir.is_dir() {
        rust_files(&dir, &mut files);
    }

    let sources = collect(files, &pages_dir());
    assert!(
        !sources.is_empty(),
        "pages/{name}.rs 与 pages/{name}/ 都没扫到非测试 .rs：目录派生失效了"
    );
    sources
}

/// 单个文件，`relative` 相对 `src/`——`pages/` 树外的产品文件用这条（`app/view/instances.rs`）。
///
/// # Panics
/// 文件不存在 ⇒ panic。清单时代「找不到就 panic」的保护在这里原样保留：搬家的文件要回来改判据。
pub(crate) fn source_at(relative: &str) -> Source {
    let path = src_dir().join(relative);
    assert!(
        path.is_file(),
        "找不到 src/{relative}：它搬家了？（这条判据是它的读者之一，搬家要一起改）"
    );
    read(&path, relative)
}

/// 从派生结果里按名字取一个。
///
/// # Panics
/// 名字不在派生结果里 ⇒ panic 并列出扫到的全部名字（「搬家会自己红」靠这条）。
pub(crate) fn only<'a>(sources: &'a [Source], name: &str) -> &'a Source {
    sources
        .iter()
        .find(|source| source.name == name)
        .unwrap_or_else(|| {
            let names: Vec<&str> = sources.iter().map(|source| source.name.as_str()).collect();
            panic!("目录派生里没有 {name}（搬走了？要跟着改判据）。扫到的是：{names:?}")
        })
}

/// 文件的产品部分：**只剔掉内联用例模块**（`#[cfg(test)] mod tests { … }` 那一整段）。
///
/// 用例里写字面量 / 参考值是**应该的**（`instances/state.rs` 的内联用例就带一串 `-Xss4m` 这类夹具值），
/// 判据只该看产品那一半。
///
/// ## 为什么不是「第一个 `#[cfg(test)]` 之后全不要」（M338 r1 的 P1）
///
/// 本仓的 `#[cfg(test)]` 大多**不在文件末尾**，而夹在产品码中间：
/// `pages/instances.rs:61` 与 `pages/instances/mods.rs:53` 是 `mod tests;`（**声明**，正文在
/// `tests/` 目录里，早被 [`is_test_file`] 排除），`pages/instances/saves.rs:91/95/101` 是
/// 条件 re-export。按「第一个 `#[cfg(test)]` 一刀切到尾」会把这些文件的**产品码**踢出判据
/// （实测：instances.rs 只覆盖前 60 行、mods.rs 前 52 行、saves.rs 前 90 行）——
/// 守卫静默停止覆盖文件，正是本 mission 要消灭的那类债（复审实测：往 instances.rs 末尾写
/// `"Ready"`，被截断的版本绿、旧门禁红）。
///
/// 所以这里只认**内联模块**：`#[cfg(test)]` 后面跟的是 `mod <名字> {` 才跳过那一段，
/// 跳过的是**模块正文**（顶格 `}` 收尾，与 `layout::source::function_body` 同一约定），
/// 之前与之后的产品码都留着。`mod tests;` / `#[cfg(test)] use …` 不产生内联正文，原样保留
/// （它们本身不含字面量）。
pub(crate) fn product_part(text: &str) -> String {
    const ATTR: &str = "#[cfg(test)]";

    let mut kept = String::with_capacity(text.len());
    // `kept_from` = 还没抄走的位置；`seek` = 找下一个 `#[cfg(test)]` 的位置。两者必须分开：
    // 「不是内联模块」只是**跳过这个属性继续找**，一个字符都不该从产物里少掉。
    let mut kept_from = 0;
    let mut seek = 0;

    while let Some(found) = text[seek..].find(ATTR) {
        let at = seek + found;
        let after = &text[at + ATTR.len()..];
        let body = after.trim_start();

        // 只认 `mod <名字> {` 这一行（`mod tests;` 是声明，正文在别的文件里；`#[cfg(test)] use …` /
        // `#[cfg(test)] fn …` 也没有要跳的模块正文）。判据只看**模块头那一行**：整段文本里
        // 后面本来就到处是 `{`，用 `body.contains('{')` 会把 `mod tests;` 误判成内联模块。
        let header = body.lines().next().unwrap_or("");

        // 而且属性必须**独占一行**（行首只许有缩进）：文档注释里写一句
        // `` /// 内联用例写成 `#[cfg(test)] mod tests { … }` `` 不算属性行。
        // 没有这一条，产品文件的文档顺手一写就会被当成内联模块，把它之后的产品码整段丢掉
        // ⇒ 假绿（本 mission 要消灭的同一类债）。契约见本文件尾的
        // `product_part_only_strips_inline_test_modules`。
        let line_start = text[..at].rfind('\n').map_or(0, |index| index + 1);
        let own_line = text[line_start..at].trim().is_empty();

        let inline = own_line && header.starts_with("mod ") && header.contains('{');
        if !inline {
            seek = at + ATTR.len();
            continue;
        }

        // 模块正文的收尾：本仓模块/函数都是顶格 `}`（内层一律缩进，见
        // `layout::source::function_body` 的同一条约定）。
        kept.push_str(&text[kept_from..at]);
        let open = at + ATTR.len() + (after.len() - body.len());
        let end = body
            .find("\n}")
            .map_or(text.len(), |close| open + close + 2);
        kept_from = end;
        seek = end;
    }

    kept.push_str(&text[kept_from..]);
    kept
}

/// 用例文件：`*/tests.rs`、`*/tests/**`、`*_tests.rs`、`tests_*.rs`。
fn is_test_file(relative: &str) -> bool {
    let mut parts = relative.split('/');
    let name = parts.next_back().unwrap_or(relative);
    parts.any(|part| part == "tests")
        || name == "tests.rs"
        || name.starts_with("tests_")
        || name.ends_with("_tests.rs")
}

fn collect(mut files: Vec<PathBuf>, base: &Path) -> Vec<Source> {
    files.sort();

    files
        .into_iter()
        .filter_map(|path| {
            let name = relative(&path, base);
            (!is_test_file(&name)).then(|| read(&path, &name))
        })
        .collect()
}

/// 相对 `base` 的 POSIX 风格路径（仓库脚本与报错信息都用 `/`）。
fn relative(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or_else(|_| panic!("{} 不在 {} 下", path.display(), base.display()))
        .to_string_lossy()
        .replace('\\', "/")
}

fn read(path: &Path, name: &str) -> Source {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("读不动 {}（{name}）：{error}", path.display()));
    Source {
        name: name.to_owned(),
        text,
    }
}

fn rust_files(dir: &Path, collected: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|error| panic!("读不动 {}：{error}", dir.display()));

    for entry in entries {
        let path = entry
            .unwrap_or_else(|error| panic!("目录项读失败（{}）：{error}", dir.display()))
            .path();
        if path.is_dir() {
            rust_files(&path, collected);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            collected.push(path);
        }
    }
}

/// **契约用例**：`product_part` 只许剔掉**真的内联用例模块**那一整段。
///
/// 为什么要有这条：其余三张表的护栏全都长在 `product_part` 之上——它多切一段，护栏就
/// **静默少覆盖一段**（假绿），而且没有任何别的证据会变红。M338 r2 复审对着真函数跑了构造输入，
/// 抓到的正是这种形态：产品文件的**文档注释**里写一句
/// ``/// 内联用例写成 `#[cfg(test)] mod tests { … }` ``，旧实现会把它当成内联模块，
/// 把它之后的产品码整段丢掉。当前树零命中，所以这是**前置防御**，不是现存缺陷。
///
/// 反向也钉：不许把守卫写成「永远不剔」，否则 `instances/state.rs` 那 56 条夹具字面量会重新
/// 进白名单（假红）。
#[test]
fn product_part_only_strips_inline_test_modules() {
    // `(输入, 必须留下, 必须不留下)`；第三个为 `""` = 没有要检查的夹具。
    let cases: [(&str, &str, &str); 5] = [
        // ① `mod tests;` 是**声明**，正文在 `tests/` 目录里（早被 `is_test_file` 排除）⇒ 产品码留下
        ("#[cfg(test)]\nmod tests;\nfn p() { \"Keep\" }\n", "Keep", ""),
        // ② 条件 re-export 同理
        (
            "#[cfg(test)]\npub(super) use x::y;\nfn p() { \"Keep\" }\n",
            "Keep",
            "",
        ),
        // ③ 真的内联用例模块 ⇒ 夹具被剔掉，**模块之后**的产品码仍在
        (
            "fn a() { \"Alpha\" }\n#[cfg(test)]\nmod tests {\n    \"-Xss4m\"\n}\nfn p() { \"Keep\" }\n",
            "Keep",
            "Xss4m",
        ),
        // ④ 文档注释里**提到** `#[cfg(test)] mod tests { … }`：那不是属性行 ⇒ 产品码必须还在
        //    （去掉「属性独占一行」这条守卫，本行立刻红）
        (
            "/// 见 `#[cfg(test)] mod tests { … }`。\nfn p() { \"Keep\" }\n",
            "Keep",
            "",
        ),
        // ⑤ 两种混在一起：先文档提及、再一段产品码、最后真的内联模块 ⇒ 两段产品码都在、夹具不在
        (
            "/// 说明：`#[cfg(test)] mod tests { … }`\nfn a() { \"Alpha\" }\n\
             #[cfg(test)]\nmod tests {\n    \"-Xss4m\"\n}\nfn p() { \"Keep\" }\n",
            "Alpha",
            "Xss4m",
        ),
    ];

    for (text, kept, dropped) in cases {
        let product = product_part(text);

        assert!(
            product.contains(kept),
            "产品码被踢出判据了（{kept:?} 不见了）—— 这是假绿：\n输入：{text:?}\n产出：{product:?}"
        );
        if !dropped.is_empty() {
            assert!(
                !product.contains(dropped),
                "夹具 {dropped:?} 漏进了产品部分 —— 这是假红（`#[cfg(test)]` 的内联模块没被剔）：\n\
                 输入：{text:?}\n产出：{product:?}"
            );
        }
    }
}
