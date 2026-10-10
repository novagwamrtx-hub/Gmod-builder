use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB * KB {
        format!("{:.2} GB", b / (KB * KB * KB))
    } else if b >= KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.0} KB", b / KB)
    }
}

pub fn count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// How long ago `then` was, e.g. "3 days ago".
pub fn ago(now: u64, then: u64) -> String {
    const DAY: u64 = 24 * 60 * 60;
    let days = now.saturating_sub(then) / DAY;
    let (n, unit) = match days {
        0 => return "today".to_string(),
        1..=59 => (days, "day"),
        60..=729 => (days / 30, "month"),
        _ => (days / 365, "year"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(size(20_686_053), "19.7 MB");
        assert_eq!(size(3 * 1024 * 1024 * 1024), "3.00 GB");
        assert_eq!(count(2_278_408), "2.3M");
        assert_eq!(ago(100, 100), "today");
        assert_eq!(ago(86_400, 0), "1 day ago");
        assert_eq!(ago(800 * 86_400, 0), "2 years ago");
    }
}
