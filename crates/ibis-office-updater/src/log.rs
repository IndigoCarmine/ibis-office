// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `updater.log`: one line per step, appended by the app (checking and
//! downloading) and by ibisOfficeUpdater (replacing), so that a customer
//! whose update keeps failing can send it:
//!
//! `2026-10-12T09:40:00Z 0.9.0 install ok moved the old copy aside`
//!
//! time (UTC), the version being installed, the step, `start`/`ok`/`FAIL`/`info`, the details
//! (the error's text for a failure). Past 1 MB the file is thrown away and
//! started again.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The size past which the log is thrown away.
pub const MAX_BYTES: u64 = 1024 * 1024;

/// How a step went.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Start,
    Ok,
    Fail,
    Info,
}

impl Status {
    fn word(self) -> &'static str {
        match self {
            Status::Start => "start",
            Status::Ok => "ok",
            Status::Fail => "FAIL",
            Status::Info => "info",
        }
    }
}

/// A log file and the version its lines are about.
#[derive(Clone, Debug)]
pub struct Log {
    path: PathBuf,
    version: String,
}

impl Log {
    pub fn new(path: impl Into<PathBuf>, version: impl Into<String>) -> Log {
        Log { path: path.into(), version: version.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append a line. A log that cannot be written is not worth stopping an update for: the error goes to stderr.
    pub fn line(&self, step: &str, status: Status, detail: &str) {
        let text = format_line(&timestamp(), &self.version, step, status, detail);
        if let Err(err) = self.append(&text) {
            eprintln!("ibisOfficeUpdater: cannot write {}: {err}", self.path.display());
        }
    }

    pub fn start(&self, step: &str, detail: &str) {
        self.line(step, Status::Start, detail)
    }

    pub fn ok(&self, step: &str, detail: &str) {
        self.line(step, Status::Ok, detail)
    }

    pub fn fail(&self, step: &str, detail: &str) {
        self.line(step, Status::Fail, detail)
    }

    pub fn info(&self, step: &str, detail: &str) {
        self.line(step, Status::Info, detail)
    }

    fn append(&self, text: &str) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if std::fs::metadata(&self.path).is_ok_and(|m| m.len() > MAX_BYTES) {
            std::fs::remove_file(&self.path)?;
        }
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        file.write_all(text.as_bytes())
    }
}

/// One log line (with its newline); the details on one line, at most 1000 characters.
pub fn format_line(time: &str, version: &str, step: &str, status: Status, detail: &str) -> String {
    let one_line: String = detail.split_whitespace().collect::<Vec<_>>().join(" ");
    let detail: String = if one_line.chars().count() > 1000 { one_line.chars().take(1000).collect::<String>() + "..." } else { one_line };
    let version = if version.is_empty() { "-" } else { version };
    format!("{time} {version} {step} {} {detail}\n", status.word()).replace(" \n", "\n")
}

/// The time now as `2026-10-12T09:40:00Z` (UTC).
#[allow(clippy::disallowed_methods)] // native only: the web version has no updater
pub fn timestamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    utc(secs)
}

/// `secs` since 1970-01-01 as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc(secs: u64) -> String {
    let (days, rest) = (secs / 86400, secs % 86400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(1_781_257_200), "2026-06-12T09:40:00Z");
        assert_eq!(utc(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn a_line_is_one_line() {
        assert_eq!(format_line("T", "0.9.0", "install", Status::Fail, "a\nb  c"), "T 0.9.0 install FAIL a b c\n");
        assert_eq!(format_line("T", "", "check", Status::Ok, ""), "T - check ok\n");
    }

    #[test]
    fn past_a_megabyte_the_log_starts_again() {
        let dir = std::env::temp_dir().join(format!("ibis-updater-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = Log::new(dir.join("Logs/updater.log"), "0.9.0");
        log.ok("a", "first");
        std::fs::write(log.path(), vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        log.ok("b", "second");
        let text = std::fs::read_to_string(log.path()).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains(" b ok second"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
