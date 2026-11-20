#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warning,
    Success,
    Info,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogLine {
    pub level: LogLevel,
    pub text: String,
}

impl LogLine {
    /// A line printed by the restore script or pg_restore, coloured by its content.
    pub fn from_output(text: String) -> Self {
        Self {
            level: classify_log_line(&text),
            text,
        }
    }

    pub fn note(level: LogLevel, text: impl Into<String>) -> Self {
        Self {
            level,
            text: text.into(),
        }
    }
}

// Same rules as the terminal colouring used with the original scripts.
fn classify_log_line(line: &str) -> LogLevel {
    let lowercase_line = line.to_lowercase();
    if lowercase_line.contains("error") {
        return LogLevel::Error;
    }
    if lowercase_line.contains("warning") {
        return LogLevel::Warning;
    }
    if lowercase_line.contains("success") || lowercase_line.contains("completed") {
        return LogLevel::Success;
    }
    LogLevel::Info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_uses_keywords_with_error_first() {
        assert_eq!(
            classify_log_line("pg_restore: error: could not connect"),
            LogLevel::Error
        );
        assert_eq!(
            classify_log_line("WARNING: errors ignored on restore: 2"),
            LogLevel::Error
        );
        assert_eq!(
            classify_log_line("pg_restore: warning: no owner"),
            LogLevel::Warning
        );
        assert_eq!(
            classify_log_line(" RESTORE COMPLETED SUCCESSFULLY"),
            LogLevel::Success
        );
        assert_eq!(
            classify_log_line("pg_restore: creating TABLE public.users"),
            LogLevel::Info
        );
    }
}
