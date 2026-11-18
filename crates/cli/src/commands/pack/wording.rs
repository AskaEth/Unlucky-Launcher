//! CLI 那一壳两条**跨命令**的人读句（M346）：一步处理器的结局、一个实例的登记结果。
//!
//! # 为什么这两句只有一份
//!
//! 它们同一件事要印三个命令：`pack import`（`commands/pack/lines.rs`）、
//! `install --loader`（`commands/install/loader_processors.rs` / `report_loader.rs`；不带
//! `--loader` 的 `install` 没有 processors 段，印不到单步结局）、`instances adopt`
//! （`commands/instances/adopt.rs`）。M346 之前每处各手写一份中文，
//! 而界面读的是目录里的 [`Key::PackProcessorStepRan`] / [`Key::PackProcessorStepSkipped`] /
//! [`Key::PackReportRegistered`] ⇒ 改一次文案，界面跟着变、CLI 一个字不动。
//! 现在这几句的**唯一实现**在本文件：判据是下面 `mod tests` 里那条源码扫描用例——needle 每次跑
//! 都从目录现取，扫 `src/` 下**除测试文件外的每一个文件整份**（生产路径上一处手写都不许有）。
//!
//! 调用点（生产路径）各四处，逐处都有"实参对不对"的用例（M346 r2 P2-1）：`registered_line` 在
//! `pack.rs`、`install/report.rs`、`install/report_loader.rs`、`instances/adopt.rs`；`step_body` 在
//! `pack/lines.rs`（两处）与 `install/loader_processors.rs`、`install/report_loader/processors.rs`
//! （各一处）。四条登记行用例分别住在 `pack/tests/m346.rs`、`install/report.rs` 与
//! `install/report_loader.rs` 的 `mod tests`、`instances/adopt.rs` 的 `mod tests`——helper 自己的
//! 用例只能证明"这串字是人话"，证明不了"调用方传对了字段"。
//!
//! # 为什么住在 `commands/pack/`（而不是 `crates/cli/src/catalog.rs`）
//!
//! `crates/cli/src/catalog.rs` 是「两处以上共用」的既有落点，这两句本该搬过去；但那条文件不在
//! M346 的 scope 里（scope 覆盖 `commands/pack/**`、`install/{report,report_loader,
//! loader_processors}.rs`、`instances/adopt.rs` 与 i18n 目录）。跨命令共用要求模块可见性
//! `pub(crate)`，而三个候选落点里只有 `commands/pack.rs` 能在本 mission 里改（`install` 与
//! `instances` 的 `mod` 声明在各自的父文件里，都不在 scope）。**已登记 finding 让它搬回
//! `cli/src/catalog.rs`**——搬移是纯搬移，调用点路径跟着改一行。

use unlucky_i18n::Key;

use crate::catalog::CATALOG;

/// 一步处理器结局的人读后半句（`处理器 1/2 <jar> → {这里}`）。
///
/// `status` 是 core 的稳定 id（`executed` / `skipped`，见
/// `core::addon::loader::processors::StepStatus::as_str`）：两处生产来源分别是
/// `StepOutcome.status`（`pack import` 的实时行）与报告里的 `ProcessorStep.status`（`&'static str`），
/// 只有字符串这一个公共面，所以这里收 `&str` 而不是某个壳自己的枚举。
///
/// 不是 `skipped` 就按 `executed` 出词（与 M346 之前的分支形状一致：core 只有这两档）。
///
/// # 两档各说一件事（口径与界面 `packui/model/report_processor.rs` 同源）
///
/// 跑过的那步印产物数与耗时（真起过 java，用户想知道花了多久）；跳过那步整句就是
/// [`Key::PackProcessorStepSkipped`]，**不印产物数与耗时**（跳过的耗时恒为 0，印一个 0 只会误导；
/// 产物个数在 `--json` 的 `processors.steps[].outputs` 里照旧给脚本）。
pub(crate) fn step_body(status: &str, outputs: usize, elapsed_ms: u128) -> String {
    if status == "skipped" {
        return CATALOG.text(Key::PackProcessorStepSkipped).to_owned();
    }
    CATALOG.render(
        Key::PackProcessorStepRan,
        &[
            ("outputs", &outputs.to_string()),
            // 纯数字毫秒：单位在目录那句里（`{elapsed} ms`），带进来就上屏 `1240 ms ms`。
            ("elapsed", &elapsed_ms.to_string()),
        ],
    )
}

/// 「已登记实例 …」那一行（`pack import` / `install` / `instances adopt` 同一份）。
///
/// 前一段取目录里 [`Key::PackReportRegistered`]（界面读的**同一句**），所以那句的措辞改一个字，
/// 两个壳一起变；后两段（实例 id / 版本 id）是 CLI 独有的细节，界面拿不到它们
/// （`presenter` 的 `ImportReportView.registered` 只带一个名字），所以仍由这里手写。
///
/// 为什么本 mission **不把 `{id}` / `{version}` 扩进目录那句**：`Catalog::render` 对没给的占位符
/// **原样留字**（`catalog/tests.rs` 的 `render_substitutes_and_keeps_unfilled_placeholders_visible`），
/// 扩了占位符而界面那一侧不补参数，GUI 就会画出 `Registered instance Survival (id {id})`；
/// 而界面要补参数得先让 DTO 带上 id 与版本，那要改 `presenter` 与 `app-iced`——都不在本 mission 的
/// scope 里。差异清单写在 M346 的报告里，扩占位是下一轮的活（届时两侧一起改、旧调用点不红）。
///
/// # 只有「复用」时才多说半句
///
/// `created == false`（这次复用已有档案）才追加 `·复用已有档案`：新建是常态，给常态加一个
/// `（新建）` 只是噪音。`instances adopt` 的 `created` 恒真（core 会先拒绝同名同版本的重登记），
/// 于是那一行天然不带这半句——不为一个恒假的判断编一条没人走的分支。
pub(crate) fn registered_line(name: &str, id: &str, version: &str, created: bool) -> String {
    let mut line = format!(
        "{}（id {id}）→ 版本 {version}",
        CATALOG.render(Key::PackReportRegistered, &[("name", name)])
    );
    if !created {
        line.push_str("·复用已有档案");
    }
    line
}

/// `--dry-run` 那一行：这两条命令在这一档**压根没登记实例**，整句替掉登记行。
///
/// 同一句原先在 `install` 的两条出口（`report.rs` / `report_loader.rs`）各手写了一遍
/// （M346 r2 顺手收）；它不是目录 key——目录里没有"没登记"这种状态，界面那侧也不需要它
/// （GUI 的干跑不画这类收尾行）。
pub(crate) const DRY_RUN_UNREGISTERED: &str = "（--dry-run：未下载任何文件，未登记实例）";

#[cfg(test)]
mod tests {
    //! 判据：**断言用户会看到的那串字**（字面量），不是 `CATALOG.render(..)` 自证。
    //!
    //! 为什么字面量重要：写 `step_body(..) == CATALOG.render(..)` 时，把目录里那句改掉两边一起变，
    //! 用例永远绿——那只证明"读了同一个函数"，证明不了"那句话还是人话"。这里钉的是屏幕上的字，
    //! 所以改目录就会红（M346 的变异：把 `执行完成` 改成 `执行完了` ⇒ 本文件三条用例红）。

    use super::*;

    /// 两档结局各读目录里的哪一句（**字面量**）。
    ///
    /// 抓得住什么（变异判据）：
    /// - 改目录 `PackProcessorStepRan` 的任意一个字（`执行完成` → `执行完了`、把 `·` 改成 `，`）
    ///   ⇒ 第一条红，报文里同时给出「现在渲染成什么」与「这条用例钉的是什么」；
    /// - 改 `PackProcessorStepSkipped` 的一个字（`跳过` → `略过`）⇒ 第二条红；
    /// - 把 `step_body` 退回 M346 之前的手写中文（`执行完成` + `（产物 N 个，T ms）` 那套排版）
    ///   ⇒ 前两条都红；
    /// - 跳过那档又开始印产物数 / 耗时 ⇒ 第二条红；
    /// - 耗时把单位带进数字（上屏 `1240 ms ms`）⇒ 第一条红。
    #[test]
    fn both_step_outcomes_come_from_the_catalog() {
        assert_eq!(
            step_body("executed", 3, 1_240),
            "执行完成 · 产物 3 个 · 1240 ms",
            "目录 PackProcessorStepRan 那句变了"
        );
        assert_eq!(
            step_body("skipped", 2, 0),
            "跳过 · 产物已就绪",
            "目录 PackProcessorStepSkipped 那句变了（跳过的步不许再印产物数与耗时）"
        );
    }

    /// 「已登记实例」那行：**前缀读目录**（与界面同一句），后两段是 CLI 独有的细节。
    ///
    /// 抓得住什么（变异判据）：
    /// - 改目录 `PackReportRegistered` 的一个字（`已登记实例` → `已登记账号`）⇒ 第一条红
    ///   （前缀不是手写的，所以它跟着目录变）；
    /// - 把前缀改回手写 `已登记实例 {name}` ⇒ 目录改字时本条不红 ⇒ 抄回了 M346 修的那条漂移通道；
    /// - `created` 的判断写反（新建也说「复用已有档案」）⇒ 第二条红；
    /// - 丢掉 id 或版本那个占位 ⇒ 第一条红（少一段用户就分不清登记的是哪一个实例）。
    #[test]
    fn the_registered_line_starts_with_the_catalog_sentence() {
        assert_eq!(
            registered_line("Survival", "3f2a1c", "1.20.1-forge-47.2.0", true),
            "已登记实例 Survival（id 3f2a1c）→ 版本 1.20.1-forge-47.2.0",
            "前缀取自目录 PackReportRegistered，后两段是 CLI 的细节"
        );
        assert_eq!(
            registered_line("Survival", "3f2a1c", "1.20.1-forge-47.2.0", false),
            "已登记实例 Survival（id 3f2a1c）→ 版本 1.20.1-forge-47.2.0·复用已有档案",
            "复用已有档案时多说半句（新建是常态，不加噪音）"
        );
    }

    /// 三个命令的四个调用点都走这一份：**判据是「只有一处实现」**，不是「两边结论一致」。
    ///
    /// # 不变量（一句话）
    ///
    /// **所有非测试文件里，目录里那三句话（及其两个退役排版）一个字符都不许出现**——needle 每次
    /// 跑用例时从 [`CATALOG`] 现取，退役排版只当固定哨兵；豁免只有一种，就是"这个文件整份是测试"。
    ///
    /// # 为什么按**路径**豁免，而不是"文件内位置"
    ///
    /// 前四轮都在推"哪几行算代码"：r2 按文件白名单（把三个生产文件整份放行）、r3 按「第一个
    /// `#[cfg(test)]` 之前」切、r4/r5 按 item 边界与括号深度推。每一种都被下面这类完全合法的
    /// 形状打穿：`#[cfg(test)]` 只 gate 一行再导出（`pack/lines.rs:40`）、rustfmt 接受的多行数组
    /// 与多行 `use`、以及本仓今天就有的**跨行字符串**文件。所以 r6 把边界推算整块删掉：一个文件
    /// 要么整份是测试（路径认得出来），要么整份算生产，没有中间态。
    ///
    /// 代价：生产文件里的**内联** `mod tests` 不得断言这几句 ⇒ r6 把 `install/report.rs`、
    /// `install/report_loader.rs`、`instances/adopt.rs` 的测试模块搬进了各自的 `tests/` 子目录
    /// （纯搬移，`#[path] mod tests;` 保持调用面不变）。
    ///
    /// 抓得住什么：非测试文件里那几句出现**任何位置**（句首、句中、`format!` 里、注释里）⇒ 红，
    /// 报文带 `文件:行`；抓不住：`--json` 里那些字段（本文件不碰数据层）。
    #[test]
    fn no_cli_source_file_hand_writes_these_sentences_again() {
        // needle **从目录现取**：每个 key 模板里第一个 `{` 之前那段（`已登记实例 {name}` ⇒
        // `已登记实例`），再补两条**已退役的排版**当固定哨兵。不能手抄（r4 就是手抄的）：目录把
        // 「跳过（产物已就绪）」改成「跳过 · 产物已就绪」之后，手抄的 needle 对今天那句直接失明。
        // **不加前引号**（r5 加过）：句子排在字面量中间（把 body 内联回 `format!`）才是最常见的
        // 回归形态，加了引号只认"从引号起头"的那种，恰好漏掉它。
        let needles: Vec<String> = [
            Key::PackProcessorStepRan,
            Key::PackProcessorStepSkipped,
            Key::PackReportRegistered,
        ]
        .into_iter()
        .map(|key| {
            CATALOG
                .text(key)
                .split('{')
                .next()
                .unwrap_or_default()
                .trim_end()
                .to_owned()
        })
        .chain(["跳过（产物已就绪）".to_string(), "执行完成".to_string()])
        .collect();
        let this_file = file!().replace('\\', "/");
        let mut offenders = Vec::new();
        for entry in walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")) {
            let relative = entry.to_string_lossy().replace('\\', "/");
            if is_test_only(&relative, &this_file) {
                continue;
            }
            let text = match std::fs::read_to_string(&entry) {
                Ok(text) => text,
                Err(_) => continue,
            };
            for (index, line) in text.lines().enumerate() {
                for needle in &needles {
                    if line.contains(needle.as_str()) {
                        offenders.push(format!("{relative}:{}：{needle}", index + 1));
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "这几句只许由 `wording.rs` 渲染，非测试文件里一个字符都不许出现：{offenders:?}"
        );
    }

    /// 整份就是测试的文件：`tests.rs` / `tests/**.rs` / `*_tests.rs`，外加守卫自己（它的断言
    /// 与 needle 就写着这些字）。**豁免只有这一条规则**，不看文件里哪一行是代码。
    fn is_test_only(relative: &str, this_file: &str) -> bool {
        if relative.ends_with(this_file) {
            return true;
        }
        let name = relative.rsplit('/').next().unwrap_or_default();
        relative.contains("/tests/")
            || relative.ends_with("/tests.rs")
            || name.ends_with("_tests.rs")
    }

    /// `src/` 下的全部 `.rs`。
    fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut seen = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return seen;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                seen.extend(walk(&path));
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                seen.push(path);
            }
        }
        seen
    }
}
