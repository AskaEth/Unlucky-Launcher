//! sink 组合测试（复合 sink 是接 M15 采集器 / UI 日志页的接缝）。

use std::sync::{Arc, Mutex};

use crate::prelude::*;

fn line(stream: Stream, text: &str) -> LogLine {
    LogLine::new(stream, text)
}

#[test]
fn composite_sink_broadcasts_in_registration_order() {
    let order = Arc::new(Mutex::new(Vec::<String>::new()));
    let first = {
        let order = order.clone();
        FnSink::new(move |line: &LogLine| {
            order.lock().unwrap().push(format!("first:{}", line.text));
        })
    };
    let second = {
        let order = order.clone();
        FnSink::new(move |line: &LogLine| {
            order.lock().unwrap().push(format!("second:{}", line.text));
        })
    };
    let composite = CompositeSink::new()
        .with(first.shared())
        .with(second.shared());
    assert_eq!(composite.len(), 2);
    composite.on_line(&line(Stream::Stdout, "hello"));

    let recorded = order.lock().unwrap().clone();
    assert_eq!(recorded, vec!["first:hello", "second:hello"]);
}

#[test]
fn composite_sink_passes_stream_and_text() {
    let seen = Arc::new(Mutex::new(Vec::<(Stream, String)>::new()));
    let recorder = {
        let seen = seen.clone();
        FnSink::new(move |line: &LogLine| {
            seen.lock().unwrap().push((line.stream, line.text.clone()));
        })
    };
    // 模拟 diagnose 侧：只记文本 + 流标记（不解析 jsonl、不 import launch 类型）。
    let mut composite = CompositeSink::new();
    composite.push(recorder.shared());
    composite.on_line(&line(Stream::Stdout, "out"));
    composite.on_line(&line(Stream::Stderr, "err"));

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], (Stream::Stdout, "out".to_string()));
    assert_eq!(seen[1], (Stream::Stderr, "err".to_string()));
}

#[test]
fn counting_sink_counts_per_stream() {
    let counter = Arc::new(CountingSink::new());
    counter.on_line(&line(Stream::Stdout, "a"));
    counter.on_line(&line(Stream::Stdout, "b"));
    counter.on_line(&line(Stream::Stderr, "c"));
    assert_eq!(counter.stdout_lines(), 2);
    assert_eq!(counter.stderr_lines(), 1);
    assert_eq!(counter.total_lines(), 3);
}

#[test]
fn empty_composite_is_harmless() {
    let composite = CompositeSink::new();
    assert!(composite.is_empty());
    composite.on_line(&line(Stream::Stdout, "ignored"));
}
