use chrono::{DateTime, Local};

use crate::model::Unit;

/// Formats a number with at most `decimals` decimals and no trailing zeros.
pub fn num(v: f64, decimals: usize) -> String {
    // Normalise -0.0 (e.g. the sum of an empty list) so it never prints as "-0".
    let v = if v == 0.0 { 0.0 } else { v };
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Weight shown for input fields: empty for zero so the hint shows.
pub fn weight_input(kg: f64, unit: Unit) -> String {
    if kg <= 0.0 {
        String::new()
    } else {
        num(unit.from_kg(kg), 2)
    }
}

pub fn weight(kg: f64, unit: Unit) -> String {
    format!("{} {}", num(unit.from_kg(kg), 1), unit.label())
}

/// Large volumes: 12 340 kg -> "12.3k kg".
pub fn volume(kg: f64, unit: Unit) -> String {
    let v = unit.from_kg(kg);
    if v >= 10_000.0 {
        format!("{}k {}", num(v / 1000.0, 1), unit.label())
    } else {
        format!("{} {}", num(v, 0), unit.label())
    }
}

pub fn set(kg: f64, reps: u32, unit: Unit) -> String {
    if kg > 0.0 {
        format!("{} × {}", num(unit.from_kg(kg), 2), reps)
    } else {
        format!("{reps} reps")
    }
}

/// "1 set", "3 sets".
pub fn count(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// Stopwatch style: 5:07, 1:02:03.
pub fn clock(secs: i64) -> String {
    let s = secs.max(0);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Human duration: 42m, 1h 05m.
pub fn duration(secs: i64) -> String {
    let m = secs.max(0) / 60;
    if m >= 60 {
        format!("{}h {:02}m", m / 60, m % 60)
    } else {
        format!("{m}m")
    }
}

fn local(ts: i64) -> Option<DateTime<Local>> {
    DateTime::from_timestamp(ts, 0).map(|d| d.with_timezone(&Local))
}

pub fn date_time(ts: i64) -> String {
    local(ts).map(|d| d.format("%a %-d %b %Y · %H:%M").to_string()).unwrap_or_default()
}

pub fn short_date(ts: i64) -> String {
    local(ts).map(|d| d.format("%-d %b").to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(num(80.0, 2), "80");
        assert_eq!(volume(-0.0, Unit::Kg), "0 kg");
        assert_eq!(count(1, "set"), "1 set");
        assert_eq!(count(3, "set"), "3 sets");
        assert_eq!(num(82.5, 2), "82.5");
        assert_eq!(weight_input(0.0, Unit::Kg), "");
        assert_eq!(weight_input(100.0, Unit::Lb), "220.46");
        assert_eq!(volume(12_340.0, Unit::Kg), "12.3k kg");
        assert_eq!(set(0.0, 12, Unit::Kg), "12 reps");
        assert_eq!(set(60.0, 8, Unit::Kg), "60 × 8");
        assert_eq!(clock(307), "5:07");
        assert_eq!(clock(3723), "1:02:03");
        assert_eq!(duration(3900), "1h 05m");
        assert_eq!(duration(59), "0m");
    }
}
