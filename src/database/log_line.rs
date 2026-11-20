use crate::constants::LOG_TOOL_PREFIXES;
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

/// The restore log: the lines kept in memory plus how many lines arrived in total.
/// pg_restore --verbose prints a line per object, so long restores are trimmed, but
/// errors and warnings are never dropped and the total stays exact.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RestoreLog {
    lines: Vec<LogLine>,
    received_line_count: usize,
}

impl RestoreLog {
    pub fn starting_with(first_line: LogLine) -> Self {
        Self {
            lines: vec![first_line],
            received_line_count: 1,
        }
    }

    /// Adds a line, then drops the oldest info lines while more than `max_kept_lines` are kept.
    pub fn push(&mut self, line: LogLine, max_kept_lines: usize) {
        self.lines.push(line);
        self.received_line_count += 1;
        while self.lines.len() > max_kept_lines {
            let oldest_info = self
                .lines
                .iter()
                .position(|kept| kept.level == LogLevel::Info);
            self.lines.remove(oldest_info.unwrap_or(0));
        }
    }

    pub fn lines(&self) -> &[LogLine] {
        &self.lines
    }

    pub fn received_line_count(&self) -> usize {
        self.received_line_count
    }

    pub fn trimmed_line_count(&self) -> usize {
        self.received_line_count - self.lines.len()
    }

    /// Exact for errors and warnings, which are never trimmed.
    pub fn count(&self, level: LogLevel) -> usize {
        self.lines.iter().filter(|line| line.level == level).count()
    }
}

/// pg_restore and psql start lines with their own severity ("pg_restore: warning: …",
/// "ERROR: …"). That label wins over words later in the message, so
/// "pg_restore: warning: errors ignored on restore: 8" is a warning, not an error.
fn classify_log_line(line: &str) -> LogLevel {
    let lowercase_line = line.trim().to_lowercase();
    let message = LOG_TOOL_PREFIXES
        .iter()
        .find_map(|prefix| lowercase_line.strip_prefix(prefix))
        .unwrap_or(&lowercase_line);
    if message.starts_with("error") {
        return LogLevel::Error;
    }
    if message.starts_with("warning") {
        return LogLevel::Warning;
    }

    // Other lines: the keyword rules of the original terminal colouring.
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
            classify_log_line("pg_restore: warning: errors ignored on restore: 8"),
            LogLevel::Warning
        );
        assert_eq!(
            classify_log_line("ERROR: Could not recreate app_db."),
            LogLevel::Error
        );
        assert_eq!(
            classify_log_line("psql: error: connection refused"),
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

    #[test]
    fn log_counts_every_line_but_keeps_errors_and_warnings() {
        let mut log = RestoreLog::starting_with(LogLine::note(LogLevel::Error, "first error"));
        for text in ["old info", "warning: careful", "new info"] {
            log.push(LogLine::from_output(text.to_string()), 3);
        }

        let kept: Vec<&str> = log.lines().iter().map(|line| line.text.as_str()).collect();
        assert_eq!(kept, vec!["first error", "warning: careful", "new info"]);
        assert_eq!(log.received_line_count(), 4);
        assert_eq!(log.trimmed_line_count(), 1);
        assert_eq!(
            (log.count(LogLevel::Error), log.count(LogLevel::Warning)),
            (1, 1)
        );
    }
}
