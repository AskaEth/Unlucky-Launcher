//! 环形缓冲 / 级别猜测 / 窗口检测。

use crate::prelude::*;

fn line(stream: Stream, text: &str) -> LogLine {
    LogLine::new(stream, text)
}

#[test]
fn ring_buffer_respects_line_limit() {
    let mut buffer = RingBuffer::new(RingBufferConfig {
        max_lines: 3,
        max_bytes: 1024 * 1024,
    });
    for index in 0..5 {
        buffer.push(line(Stream::Stdout, &format!("line-{index}")));
    }
    assert_eq!(buffer.len(), 3);
    let texts: Vec<String> = buffer.lines().map(|item| item.text.clone()).collect();
    assert_eq!(texts, vec!["line-2", "line-3", "line-4"]);
    assert_eq!(buffer.total_pushed(), 5);
}

#[test]
fn ring_buffer_keeeps_key_lines_after_eviction() {
    let mut buffer = RingBuffer::new(RingBufferConfig {
        max_lines: 2,
        max_bytes: 1024 * 1024,
    });
    buffer.push(line(
        Stream::Stderr,
        "java.lang.ExceptionInInitializerError",
    ));
    buffer.push(line(Stream::Stdout, "noise-1"));
    buffer.push(line(Stream::Stdout, "noise-2"));
    buffer.push(line(Stream::Stdout, "noise-3"));

    let texts: Vec<String> = buffer.lines().map(|item| item.text.clone()).collect();
    assert!(texts
        .iter()
        .any(|text| text.contains("ExceptionInInitializer")));
    assert!(texts.contains(&"noise-3".to_string()));
    assert!(!texts.contains(&"noise-1".to_string()));
    assert_eq!(buffer.error_lines().len(), 1);
}

#[test]
fn ring_buffer_respects_byte_limit() {
    let mut buffer = RingBuffer::new(RingBufferConfig {
        max_lines: 1000,
        max_bytes: 32,
    });
    buffer.push(line(Stream::Stdout, "0123456789"));
    buffer.push(line(Stream::Stdout, "abcdefghij"));
    buffer.push(line(Stream::Stdout, "ABCDEFGHIJ"));
    assert!(buffer.len() < 4, "字节上限应触发淘汰");
    assert!(buffer.text().contains("ABCDEFGHIJ"));
}

#[test]
fn level_guessing_matches_hmcl_rules() {
    assert_eq!(
        log_level::guess(false, "[12:34:56] [Render thread/ERROR]: boom"),
        Level::Error
    );
    assert_eq!(
        log_level::guess(false, "[12:34:56] [main/INFO]: hi"),
        Level::Info
    );
    assert_eq!(
        log_level::guess(false, "[12:34:56] [Worker-Main-1/WARN] [mixin/]: careful"),
        Level::Warn
    );
    assert_eq!(log_level::guess(false, "[SEVERE] java error"), Level::Error);
    assert_eq!(log_level::guess(false, "plain text"), Level::Unknown);
    // stderr 一律当错误，authlib-injector 例外。
    assert_eq!(log_level::guess(true, "anything"), Level::Error);
    assert_eq!(
        log_level::guess(true, "[authlib-injector] loaded"),
        Level::Info
    );
}

#[test]
fn line_level_uses_stream_and_text() {
    assert_eq!(line(Stream::Stderr, "boom").level(), Level::Error);
    assert_eq!(
        line(Stream::Stdout, "[12:34:56] [main/DEBUG]: detail").level(),
        Level::Debug
    );
}

#[test]
fn window_detector_reports_heuristic_once() {
    let mut detector = WindowDetector::new();
    assert!(detector
        .observe("[12:00:00] [main/INFO]: Loading LWJGL")
        .is_none());
    let detection = detector
        .observe("[12:00:01] [Render thread/INFO]: LWJGL Version: 3.3.1")
        .expect("应命中 lwjgl version");
    assert_eq!(detection.heuristic, "lwjgl_version");
    assert!(detection.evidence.contains("LWJGL Version"));
    assert!(detector.is_detected());
    // 只报一次。
    assert!(detector.observe("LWJGL Version: 3.3.1").is_none());
    assert_eq!(
        detector.detection().map(|d| d.heuristic.as_str()),
        Some("lwjgl_version")
    );
}
