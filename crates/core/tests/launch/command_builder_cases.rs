//! `CommandBuilder` 默认值抑制语义测试（`analyse/02` §4.2）。

use crate::prelude::*;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn add_default_suppresses_when_prefix_exists() {
    let mut builder = CommandBuilder::new();
    builder.add("-Xmx8G");
    let existing = builder.add_default("-Xmx", "4096m");
    assert_eq!(existing.as_deref(), Some("-Xmx8G"));
    assert_eq!(builder.args(), &strings(&["-Xmx8G"]));

    let added = builder.add_default("-Xms", "1024m");
    assert_eq!(added, None);
    assert_eq!(builder.args(), &strings(&["-Xmx8G", "-Xms1024m"]));
}

#[test]
fn unstable_flags_do_not_conflict() {
    let mut builder = CommandBuilder::new();
    builder.add("-XX:-UseG1GC");
    // 用户已经用 `-XX:-UseG1GC` 关掉了 G1，就不再补 `-XX:+UseG1GC`。
    assert!(builder.add_unstable_flag("UseG1GC", true).is_some());
    assert!(!builder.contains_exact("-XX:+UseG1GC"));

    let mut builder = CommandBuilder::new();
    builder.add("-XX:MaxInlineLevel=20");
    assert!(builder
        .add_unstable_default("MaxInlineLevel", "15")
        .is_some());
    assert_eq!(builder.args(), &strings(&["-XX:MaxInlineLevel=20"]));
}

#[test]
fn add_all_default_handles_each_shape() {
    let mut builder = CommandBuilder::new();
    builder.add("-Dfile.encoding=GBK");
    builder.add("-XX:+UseSerialGC");
    builder.add("-Xmx8G");
    builder.add("-cp");
    builder.add("user-cp");

    builder.add_all_default(&strings(&[
        "-Dfile.encoding=UTF-8",
        "-Dsun.stdout.encoding=UTF-8",
        "-XX:+UseG1GC",
        "-XX:G1MixedGCCountTarget=5",
        "-Xmx4096m",
        "-Xss1m",
        "-cp",
        "default-cp",
        "exact-arg",
    ]));

    assert_eq!(
        builder.args(),
        &strings(&[
            "-Dfile.encoding=GBK",
            "-XX:+UseSerialGC",
            "-Xmx8G",
            "-cp",
            "user-cp",
            "-Dsun.stdout.encoding=UTF-8",
            "-XX:+UseG1GC",
            "-XX:G1MixedGCCountTarget=5",
            "-Xss1m",
            "exact-arg",
        ])
    );
}

#[test]
fn argfile_expansion_suppresses_later_defaults() {
    let mut builder = CommandBuilder::new();
    builder.add_all_without_parsing_and_read_external(
        &strings(&["@options.txt"]),
        &|path: &str| {
            if path == "options.txt" {
                Some("-Xmx12G\n-Dfile.encoding=COMPAT\n".to_string())
            } else {
                None
            }
        },
    );
    // @argfile 里的 -Xmx12G 已登记为外部参数，默认值必须被抑制。
    assert!(builder.add_default("-Xmx", "4096m").is_some());
    assert!(builder.add_default("-Dfile.encoding=", "UTF-8").is_some());
    // 外部参数只参与判重，不进 argv。
    assert!(!builder.contains_exact("-Xmx12G"));
    assert_eq!(builder.args(), &strings(&["@options.txt"]));
}

#[test]
fn tokenize_handles_quotes_and_escapes() {
    assert_eq!(
        command_builder::tokenize(r#"--username "a b" --path 'C:\x y' plain"#),
        strings(&["--username", "a b", "--path", "C:\\x y", "plain"])
    );
    assert_eq!(
        command_builder::tokenize(r#"-Dmsg="say \"hi\"""#),
        strings(&[r#"-Dmsg=say "hi""#])
    );
    assert!(command_builder::tokenize("   ").is_empty());
}

#[test]
fn parse_argfile_ignores_comments_and_blank_lines() {
    let contents = "# 注释\n-Xmx2G\n\n-Xms512m -Dfoo=bar\n";
    assert_eq!(
        command_builder::parse_argfile(contents),
        strings(&["-Xmx2G", "-Xms512m", "-Dfoo=bar"])
    );
}

#[test]
fn forbidden_args_are_removed_at_the_end() {
    let mut builder = CommandBuilder::new();
    builder.add_all(&strings(&["-Xincgc", "-Xmx1G", "-Xincgc"]));
    let removed = builder.remove_forbidden(&["-Xincgc"]);
    assert_eq!(removed.len(), 2);
    assert_eq!(builder.args(), &strings(&["-Xmx1G"]));
}

#[test]
fn placeholders_are_rendered_on_add_but_not_on_raw_add() {
    let mut map = unlucky_core::launch::placeholders::PlaceholderMap::new();
    map.insert("version_name", "1.20.1");
    let mut builder = CommandBuilder::with_placeholders(map);
    builder.add("--version");
    builder.add("${version_name}");
    builder.add_without_parsing("--raw ${version_name}");
    assert_eq!(
        builder.args(),
        &strings(&["--version", "1.20.1", "--raw ${version_name}"])
    );
    // 人读形式给含空格的参数加引号（日志与快照测试用）。
    assert_eq!(
        builder.display_args(),
        "--version 1.20.1 \"--raw ${version_name}\""
    );
    let mut quoted = CommandBuilder::new();
    quoted.add("-Dmsg=hello world");
    assert_eq!(quoted.display_args(), "\"-Dmsg=hello world\"");
}
