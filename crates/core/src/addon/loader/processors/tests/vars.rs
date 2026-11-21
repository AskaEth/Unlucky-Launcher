//! 变量表：三种字面量、标准变量、token 替换的边界。

use std::collections::BTreeMap;
use std::path::Path;

use crate::addon::loader::processors::spec::Datum;
use crate::addon::loader::processors::vars::{Variables, VariablesInput};
use crate::addon::loader::processors::ProcessorErrorKind;
use crate::game::layout::GameLayout;

use super::{profile, temp_root};

fn data(entries: &[(&str, &str)]) -> BTreeMap<String, Datum> {
    entries
        .iter()
        .map(|(key, value)| {
            (
                (*key).to_string(),
                Datum {
                    client: Some((*value).to_string()),
                    server: None,
                },
            )
        })
        .collect()
}

fn build<'a>(
    layout: &'a GameLayout,
    data: &'a BTreeMap<String, Datum>,
    installer: &'a Path,
    scratch: &'a Path,
) -> Variables {
    Variables::build(VariablesInput {
        data,
        layout,
        mc_version: "1.20.1",
        installer,
        scratch,
    })
    .expect("变量表应能组装")
}

#[test]
fn artifact_literals_resolve_into_the_libraries_directory() {
    let root = temp_root("vars-artifact");
    let layout = GameLayout::new(&root);
    let installer = root.join("installer.jar");
    let scratch = root.join("tmp");
    let table = data(&[("MAPPINGS", "[de.oceanlabs.mcp:mcp_config:1.20.1@zip]")]);
    let variables = build(&layout, &table, &installer, &scratch);

    let value = variables.get("MAPPINGS").expect("应有 MAPPINGS");
    assert!(
        Path::new(value).ends_with("de/oceanlabs/mcp/mcp_config/1.20.1/mcp_config-1.20.1.zip"),
        "{value}"
    );
    assert!(Path::new(value).is_absolute(), "{value}");
    assert!(
        Path::new(value).starts_with(layout.libraries_dir()),
        "{value}"
    );

    // 官方 profile 也把坐标直接写在 `args` 里（`--input [de.oceanlabs.mcp:…@zip]`），
    // 整段是坐标时同样要展开成路径。
    let arg = variables
        .expand("[de.oceanlabs.mcp:mcp_config:1.20.1@zip]")
        .expect("整段坐标应能展开");
    assert_eq!(arg, value);
}

#[test]
fn quoted_literals_lose_the_quotes() {
    let root = temp_root("vars-literal");
    let layout = GameLayout::new(&root);
    let table = data(&[("SIDE_HINT", "'client'")]);
    let variables = build(&layout, &table, &root.join("i.jar"), &root.join("tmp"));
    assert_eq!(variables.get("SIDE_HINT"), Some("client"));
}

#[test]
fn plain_paths_are_extracted_from_the_installer_into_scratch() {
    let root = temp_root("vars-embedded");
    let layout = GameLayout::new(&root);
    let installer = root.join("installer.jar");
    let scratch = root.join("tmp");
    let table = data(&[("BINPATCH", "/data/client.lzma")]);
    let variables = build(&layout, &table, &installer, &scratch);

    let value = variables.get("BINPATCH").expect("应有 BINPATCH");
    assert!(value.ends_with("data-client.lzma"), "{value}");
    assert!(
        value.starts_with(&scratch.to_string_lossy().to_string()),
        "{value}"
    );
    assert_eq!(variables.embedded().len(), 1);
    assert_eq!(variables.embedded()[0].entry, "data/client.lzma");
}

#[test]
fn server_only_entries_are_ignored_for_client_installs() {
    let root = temp_root("vars-server");
    let layout = GameLayout::new(&root);
    let variables = build(
        &layout,
        &profile().data,
        &root.join("i.jar"),
        &root.join("tmp"),
    );
    assert!(variables.get("SERVER_ONLY").is_none());
    assert!(variables.get("MAPPINGS").is_some());
}

#[test]
fn standard_variables_are_absolute_and_side_is_client() {
    let root = temp_root("vars-standard");
    let layout = GameLayout::new(&root);
    let installer = root.join("installer.jar");
    let scratch = root.join("tmp");
    let variables = build(&layout, &BTreeMap::new(), &installer, &scratch);

    assert_eq!(variables.get("SIDE"), Some("client"));
    assert_eq!(variables.get("MINECRAFT_VERSION"), Some("1.20.1"));
    for key in ["MINECRAFT_JAR", "ROOT", "INSTALLER", "LIBRARY_DIR"] {
        let value = variables.get(key).unwrap_or_else(|| panic!("缺 {key}"));
        assert!(
            Path::new(value).is_absolute(),
            "{key} 不是绝对路径：{value}"
        );
    }
    assert!(variables
        .get("MINECRAFT_JAR")
        .unwrap_or_default()
        .ends_with("minecraft.jar"));
}

#[test]
fn token_replacement_handles_quotes_and_escapes() {
    let root = temp_root("vars-expand");
    let layout = GameLayout::new(&root);
    let variables = build(
        &layout,
        &BTreeMap::new(),
        &root.join("i.jar"),
        &root.join("tmp"),
    );

    assert_eq!(
        variables.expand("--side '{SIDE}'").expect("应能展开"),
        "--side {SIDE}"
    );
    assert_eq!(variables.expand("a\\{b").expect("应能展开"), "a{b");
    assert_eq!(
        variables.expand("--side {SIDE}").expect("应能展开"),
        "--side client"
    );
}

#[test]
fn missing_or_unclosed_placeholders_are_plan_errors() {
    let root = temp_root("vars-error");
    let layout = GameLayout::new(&root);
    let variables = build(
        &layout,
        &BTreeMap::new(),
        &root.join("i.jar"),
        &root.join("tmp"),
    );

    let missing = variables
        .expand("--output {NOPE}")
        .expect_err("缺变量必须报错");
    assert_eq!(missing.kind(), ProcessorErrorKind::Plan);
    assert!(missing.message().contains("NOPE"), "{}", missing.message());

    let unclosed = variables.expand("{SIDE").expect_err("未闭合必须报错");
    assert_eq!(unclosed.kind(), ProcessorErrorKind::Plan);

    let bad_escape = variables.expand("abc\\").expect_err("悬空转义必须报错");
    assert_eq!(bad_escape.kind(), ProcessorErrorKind::Plan);
}
