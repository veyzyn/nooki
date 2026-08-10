use std::sync::LazyLock;

use chrono::{Datelike, Local, TimeZone};
use regex::Regex;

use crate::models::{LogLevel, LogLine};

static MINECRAFT_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\[(?<hour>\d{2}):(?<minute>\d{2}):(?<second>\d{2})\]\s+\[(?<source>.+?)/(?<level>INFO|WARN|ERROR|SEVERE)\]:\s?(?<message>.*)$",
    )
    .expect("Minecraft console regex is valid")
});

pub fn parse_console_line(id: String, raw: String, stderr: bool, received_at: i64) -> LogLine {
    if let Some(captures) = MINECRAFT_LINE.captures(&raw) {
        let level = match &captures["level"] {
            "WARN" => LogLevel::Warn,
            "ERROR" | "SEVERE" => LogLevel::Error,
            _ => LogLevel::Info,
        };
        let at = parsed_time(&captures, received_at);
        return LogLine {
            id,
            at,
            level,
            source: captures["source"].trim().to_owned(),
            text: captures["message"].to_owned(),
        };
    }

    let level = if stderr || raw.contains("ERROR") || raw.contains("SEVERE") {
        LogLevel::Error
    } else if raw.contains("WARN") {
        LogLevel::Warn
    } else {
        LogLevel::Info
    };
    LogLine {
        id,
        at: received_at,
        level,
        source: if stderr { "Java stderr" } else { "Server" }.into(),
        text: raw,
    }
}

fn parsed_time(captures: &regex::Captures<'_>, fallback: i64) -> i64 {
    let Some(fallback_time) = chrono::DateTime::from_timestamp_millis(fallback) else {
        return fallback;
    };
    let local = fallback_time.with_timezone(&Local);
    let hour = captures["hour"].parse::<u32>().ok();
    let minute = captures["minute"].parse::<u32>().ok();
    let second = captures["second"].parse::<u32>().ok();
    let Some((hour, minute, second)) = hour.zip(minute).zip(second).map(|((h, m), s)| (h, m, s))
    else {
        return fallback;
    };
    Local
        .with_ymd_and_hms(
            local.year(),
            local.month(),
            local.day(),
            hour,
            minute,
            second,
        )
        .single()
        .map(|time| time.timestamp_millis())
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn separates_minecraft_metadata_from_the_message() {
        let line = parse_console_line(
            "line-1".into(),
            "[07:51:15] [ServerMain/INFO]: Loaded 1585 recipes".into(),
            false,
            1_800_000_000_000,
        );

        assert_eq!(line.source, "ServerMain");
        assert_eq!(line.text, "Loaded 1585 recipes");
        assert!(matches!(line.level, LogLevel::Info));
        assert_eq!(
            chrono::DateTime::from_timestamp_millis(line.at)
                .unwrap()
                .with_timezone(&Local)
                .hour(),
            7
        );
    }

    #[test]
    fn preserves_unstructured_java_output() {
        let line = parse_console_line(
            "line-2".into(),
            "A Java warning without a Minecraft prefix".into(),
            true,
            123,
        );

        assert_eq!(line.source, "Java stderr");
        assert_eq!(line.text, "A Java warning without a Minecraft prefix");
        assert_eq!(line.at, 123);
    }
}
