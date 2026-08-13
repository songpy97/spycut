use std::{
    backtrace::Backtrace,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Once},
};

use chrono::{SecondsFormat, Utc};

const DEFAULT_MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
const MAX_MESSAGE_BYTES: usize = 4_096;
static PANIC_HOOK: Once = Once::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Info,
    Warn,
    Error,
}

impl DiagnosticLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Clone)]
pub struct DiagnosticLog {
    inner: Arc<DiagnosticLogInner>,
}

struct DiagnosticLogInner {
    log_path: PathBuf,
    marker_path: PathBuf,
    previous_session_unclean: bool,
    writer: Mutex<Option<File>>,
}

impl DiagnosticLog {
    pub fn open(app_data_dir: &Path) -> io::Result<Self> {
        Self::open_with_limit(app_data_dir, DEFAULT_MAX_LOG_BYTES)
    }

    fn open_with_limit(app_data_dir: &Path, max_log_bytes: u64) -> io::Result<Self> {
        let directory = app_data_dir.join("diagnostics");
        fs::create_dir_all(&directory)?;
        let log_path = directory.join("spycut.log");
        let previous_path = directory.join("spycut.previous.log");
        if fs::metadata(&log_path)
            .map(|metadata| metadata.len() > max_log_bytes)
            .unwrap_or(false)
        {
            if previous_path.exists() {
                fs::remove_file(&previous_path)?;
            }
            fs::rename(&log_path, &previous_path)?;
        }

        let marker_path = directory.join("session.running");
        let previous_session_unclean = marker_path.exists();
        let writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        write_marker(&marker_path)?;
        Ok(Self {
            inner: Arc::new(DiagnosticLogInner {
                log_path,
                marker_path,
                previous_session_unclean,
                writer: Mutex::new(Some(writer)),
            }),
        })
    }

    pub fn disabled(app_data_dir: &Path) -> Self {
        let directory = app_data_dir.join("diagnostics");
        Self {
            inner: Arc::new(DiagnosticLogInner {
                log_path: directory.join("spycut.log"),
                marker_path: directory.join("session.running"),
                previous_session_unclean: false,
                writer: Mutex::new(None),
            }),
        }
    }

    pub fn install_panic_hook(&self) {
        let log = self.clone();
        PANIC_HOOK.call_once(move || {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let location = info
                    .location()
                    .map(|value| format!("{}:{}:{}", value.file(), value.line(), value.column()))
                    .unwrap_or_else(|| "unknown".to_string());
                let payload = info
                    .payload()
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("non-string panic payload");
                log.record(
                    DiagnosticLevel::Error,
                    "rust_panic",
                    &format!(
                        "location={location} message={payload} backtrace={}",
                        Backtrace::force_capture()
                    ),
                );
                previous(info);
            }));
        });
    }

    pub fn record(&self, level: DiagnosticLevel, event: &str, message: &str) {
        let event = sanitize_event_name(event);
        let message = sanitize_untrusted(message);
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let Ok(mut writer) = self.inner.writer.lock() else {
            return;
        };
        let Some(writer) = writer.as_mut() else {
            return;
        };
        let _ = writeln!(writer, "{timestamp} {} {event} {message}", level.label());
        let _ = writer.flush();
    }

    pub fn previous_session_unclean(&self) -> bool {
        self.inner.previous_session_unclean
    }

    pub fn mark_clean_exit(&self) {
        self.record(DiagnosticLevel::Info, "app_exit_clean", "status=completed");
        let _ = fs::remove_file(&self.inner.marker_path);
    }

    pub fn log_path(&self) -> &Path {
        &self.inner.log_path
    }

    pub fn is_available(&self) -> bool {
        self.inner
            .writer
            .lock()
            .map(|writer| writer.is_some())
            .unwrap_or(false)
    }

    #[cfg(test)]
    fn marker_path(&self) -> &Path {
        &self.inner.marker_path
    }
}

fn write_marker(path: &Path) -> io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "started={} pid={}",
        Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        std::process::id()
    )?;
    file.flush()
}

fn sanitize_event_name(event: &str) -> String {
    let sanitized = event
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .take(80)
        .collect::<String>();
    if sanitized.is_empty() {
        "unknown_event".to_string()
    } else {
        sanitized
    }
}

fn sanitize_untrusted(message: &str) -> String {
    let mut output = Vec::new();
    let mut redacting_path_tail = false;
    for token in message.split_whitespace() {
        let boundary = matches!(
            token.trim_matches(|character: char| !character.is_ascii_alphabetic()),
            "and" | "or" | "at" | "then" | "with"
        );
        if redacting_path_tail {
            if !boundary {
                continue;
            }
            redacting_path_tail = false;
        }

        if looks_like_url(token) {
            output.push("[redacted-url]".to_string());
        } else if looks_like_absolute_path(token) || looks_like_media_filename(token) {
            output.push("[redacted-path]".to_string());
            redacting_path_tail = true;
        } else {
            output.push(token.to_string());
        }
    }
    truncate_utf8(&output.join(" "), MAX_MESSAGE_BYTES)
}

fn looks_like_url(token: &str) -> bool {
    token.contains("://") || token.starts_with("tauri:") || token.starts_with("asset:")
}

fn looks_like_absolute_path(token: &str) -> bool {
    let trimmed = token
        .trim_matches(|character| matches!(character, '"' | '\'' | '(' | '[' | '{' | ',' | ':'));
    let bytes = trimmed.as_bytes();
    trimmed.starts_with('/')
        || trimmed.starts_with("\\\\")
        || trimmed.contains("=/")
        || trimmed.contains(":/")
        || trimmed.contains("=\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
        || bytes.windows(3).any(|window| {
            window[0].is_ascii_alphabetic()
                && window[1] == b':'
                && matches!(window[2], b'\\' | b'/')
        })
}

fn looks_like_media_filename(token: &str) -> bool {
    token.to_ascii_lowercase().contains(".mp4")
}

fn truncate_utf8(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }
    let target = limit.saturating_sub('…'.len_utf8());
    let mut end = target.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn writes_and_flushes_privacy_safe_events() {
        let directory = tempdir().unwrap();
        let log = DiagnosticLog::open_with_limit(directory.path(), 1024).unwrap();
        log.record(
            DiagnosticLevel::Error,
            "frontend_error",
            r#"failed at C:\Private\Course 01.mp4 and http://localhost:1234/secret-token"#,
        );

        let content = fs::read_to_string(log.log_path()).unwrap();
        assert!(content.contains("ERROR frontend_error"));
        assert!(!content.contains("Private"));
        assert!(!content.contains("Course 01"));
        assert!(!content.contains("secret-token"));
        assert!(content.contains("[redacted-path]"));
        assert!(content.contains("[redacted-url]"));
    }

    #[test]
    fn redacts_paths_embedded_in_structured_errors_and_media_basenames() {
        let message = sanitize_untrusted(
            r#"error=/Users/private/Course.mp4 source=C:\Private\Lesson.mp4 name=Secret.mp4"#,
        );

        assert!(!message.contains("Users"));
        assert!(!message.contains("Private"));
        assert!(!message.contains("Secret"));
        assert!(message.contains("[redacted-path]"));
    }

    #[test]
    fn rotates_an_oversized_log_and_keeps_one_previous_file() {
        let directory = tempdir().unwrap();
        let diagnostics = directory.path().join("diagnostics");
        fs::create_dir_all(&diagnostics).unwrap();
        fs::write(diagnostics.join("spycut.log"), b"old log larger than limit").unwrap();

        let log = DiagnosticLog::open_with_limit(directory.path(), 8).unwrap();

        assert_eq!(
            fs::read_to_string(diagnostics.join("spycut.previous.log")).unwrap(),
            "old log larger than limit"
        );
        assert!(fs::read_to_string(log.log_path()).unwrap().is_empty());
    }

    #[test]
    fn detects_an_unclean_session_and_clean_exit_removes_the_marker() {
        let directory = tempdir().unwrap();
        let first = DiagnosticLog::open_with_limit(directory.path(), 1024).unwrap();
        assert!(!first.previous_session_unclean());
        assert!(first.marker_path().is_file());
        drop(first);

        let second = DiagnosticLog::open_with_limit(directory.path(), 1024).unwrap();
        assert!(second.previous_session_unclean());
        second.mark_clean_exit();
        assert!(!second.marker_path().exists());
    }

    #[test]
    fn truncates_untrusted_messages_without_splitting_utf8() {
        let sanitized = sanitize_untrusted(&"错".repeat(2_000));
        assert!(sanitized.len() <= MAX_MESSAGE_BYTES);
        assert!(sanitized.ends_with('…'));
    }

    #[test]
    fn disabled_log_is_safe_to_use() {
        let directory = tempdir().unwrap();
        let log = DiagnosticLog::disabled(directory.path());

        log.record(DiagnosticLevel::Error, "test", "must not panic");
        log.mark_clean_exit();

        assert!(!log.is_available());
        assert!(!log.log_path().exists());
    }
}
