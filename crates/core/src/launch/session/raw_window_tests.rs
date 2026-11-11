//! `raw_window` 的单测。

use super::*;
use crate::launch::pump::Stream;

fn line(stream: Stream, text: &str) -> LogLine {
    LogLine::new(stream, text)
}

#[test]
fn keeps_the_tail_and_counts_every_error_line() {
    let mut window = RawWindow::default();
    window.push(&line(
        Stream::Stderr,
        "java.lang.IllegalStateException: boom",
    ));
    window.push(&line(Stream::Stdout, "[main/INFO]: Setting user"));
    assert_eq!(window.error_lines(), 1);
    let (stdout, stderr) = window.facts();
    assert_eq!(stdout, vec!["[main/INFO]: Setting user".to_string()]);
    assert_eq!(
        stderr,
        vec!["java.lang.IllegalStateException: boom".to_string()]
    );
}

/// 尾窗有界：`CAPACITY` 之前的行被丢掉，但**错误行数照旧全量**。
#[test]
fn the_window_is_bounded_while_error_count_is_not() {
    let mut window = RawWindow::default();
    for index in 0..(CAPACITY + 10) {
        window.push(&line(Stream::Stdout, &format!("line {index}")));
    }
    let (stdout, _) = window.facts();
    assert_eq!(stdout.len(), CAPACITY);
    assert_eq!(stdout.first().map(String::as_str), Some("line 10"));
    assert_eq!(window.error_lines(), 0);

    for _ in 0..3 {
        window.push(&line(
            Stream::Stderr,
            "java.lang.OutOfMemoryError: Java heap space",
        ));
    }
    assert_eq!(window.error_lines(), 3);
}
