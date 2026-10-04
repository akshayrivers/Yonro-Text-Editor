//! Daily writing sessions (`P5.4`): net word deltas, goals, streaks.
//!
//! * One day's counters: `{ start_words, words_written }`. The first
//!   `record` of a day pins `start_words`; later calls set
//!   `words_written = total.saturating_sub(start_words)` (deleting never
//!   goes negative).
//! * All dates are opaque `YYYY-MM-DD` strings chosen by the caller, so
//!   tests inject dates and core never reads the clock.
//! * Streaks count consecutive `words_written > 0` days; an unwritten
//!   today does not break a streak that was alive yesterday.

use std::collections::BTreeMap;

/// One day's counters (the `sessions.json` value shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct DayRecord {
    /// Manuscript total at the day's first observation.
    #[serde(default)]
    pub start_words: usize,
    /// Net words since `start_words` (floored at `0`).
    #[serde(default)]
    pub words_written: usize,
}

/// In-memory session log: per-day counters plus the daily goal.
#[derive(Debug, Clone, Default)]
pub struct SessionLog {
    days: BTreeMap<String, DayRecord>,
    goal: usize,
}

impl SessionLog {
    /// Empty log with no goal.
    #[must_use]
    pub fn new() -> Self {
        Self {
            days: BTreeMap::new(),
            goal: 0,
        }
    }

    /// Restore from file contents (already parsed day map) plus goal.
    #[must_use]
    pub fn from_parts(days: BTreeMap<String, DayRecord>, goal: usize) -> Self {
        Self { days, goal }
    }

    /// Day map for persistence (borrowed; caller serializes).
    #[must_use]
    pub fn days(&self) -> &BTreeMap<String, DayRecord> {
        &self.days
    }

    /// Daily word goal (`0` = none).
    #[must_use]
    pub fn goal(&self) -> usize {
        self.goal
    }

    /// Set the daily word goal.
    pub fn set_goal(&mut self, words: usize) {
        self.goal = words;
    }

    /// Observe the current manuscript total for `today`.
    ///
    /// Malformed dates (not `YYYY-MM-DD`) are ignored. The first call of a
    /// day pins `start_words`; later calls refresh the net delta.
    pub fn record(&mut self, total_words: usize, today: &str) {
        if parse_day(today).is_none() {
            return;
        }
        match self.days.get_mut(today) {
            Some(day) => {
                day.words_written = total_words.saturating_sub(day.start_words);
            }
            None => {
                self.days.insert(
                    today.to_string(),
                    DayRecord {
                        start_words: total_words,
                        words_written: 0,
                    },
                );
            }
        }
    }

    /// Net words written on `today` (`0` when unobserved or malformed).
    #[must_use]
    pub fn words_today(&self, today: &str) -> usize {
        self.days.get(today).map_or(0, |day| day.words_written)
    }

    /// Consecutive writing days (`words_written > 0`).
    ///
    /// An unwritten `today` does not break the streak: counting starts from
    /// yesterday in that case.
    #[must_use]
    pub fn streak_days(&self, today: &str) -> usize {
        let mut cursor = today.to_string();
        if self.words_today(&cursor) == 0 {
            let Some(prev) = prev_day(&cursor) else {
                return 0;
            };
            cursor = prev;
        }
        let mut streak = 0usize;
        loop {
            if self.words_today(&cursor) == 0 {
                break;
            }
            streak = streak.saturating_add(1);
            let Some(prev) = prev_day(&cursor) else {
                break;
            };
            cursor = prev;
        }
        streak
    }

    /// Goal progress for `today`, clamped to `0.0..=1.0` (`0.0` when goalless).
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    pub fn progress(&self, today: &str) -> f64 {
        if self.goal == 0 {
            return 0.0;
        }
        (self.words_today(today) as f64 / self.goal as f64).clamp(0.0, 1.0)
    }
}

/// Parsed `YYYY-MM-DD` as (year, month, day); `None` when malformed.
fn parse_day(today: &str) -> Option<(i64, u32, u32)> {
    let bytes = today.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = today[0..4].parse().ok()?;
    let month: u32 = today[5..7].parse().ok()?;
    let day: u32 = today[8..10].parse().ok()?;
    if month == 0 || month > 12 || day == 0 || day > 31 {
        return None;
    }
    Some((year, month, day))
}

/// Days since civil 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let shifted_year = if month <= 2 {
        year.saturating_sub(1)
    } else {
        year
    };
    let era = shifted_year.div_euclid(400);
    let yoe = shifted_year.rem_euclid(400);
    let month_index = i64::from(month);
    let day_index = i64::from(day);
    let doy = (153_i64
        .saturating_mul(if month > 2 {
            month_index.saturating_sub(3)
        } else {
            month_index.saturating_add(9)
        })
        .saturating_add(2))
    .div_euclid(5)
    .saturating_add(day_index)
    .saturating_sub(1);
    let doe = yoe
        .saturating_mul(365)
        .saturating_add(yoe.div_euclid(4))
        .saturating_sub(yoe.div_euclid(100))
        .saturating_add(doy);
    era.saturating_mul(146_097)
        .saturating_add(doe)
        .saturating_sub(719_468)
}

/// (year, month, day) from days since civil 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days.saturating_add(719_468);
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = doe
        .saturating_sub(doe.div_euclid(1_460))
        .saturating_add(doe.div_euclid(36_524))
        .saturating_sub(doe.div_euclid(146_096))
        .div_euclid(365);
    let mut year = yoe.saturating_add(era.saturating_mul(400));
    let doy = doe.saturating_sub(
        yoe.saturating_mul(365)
            .saturating_add(yoe.div_euclid(4))
            .saturating_sub(yoe.div_euclid(100)),
    );
    let mp = doy.saturating_mul(5).saturating_add(2).div_euclid(153);
    let day = doy
        .saturating_sub(mp.saturating_mul(153).saturating_add(2).div_euclid(5))
        .saturating_add(1);
    let month = if mp < 10 {
        mp.saturating_add(3)
    } else {
        mp.saturating_sub(9)
    };
    year = year.saturating_add(i64::from(month <= 2));
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// Day before `today` (`None` when malformed).
fn prev_day(today: &str) -> Option<String> {
    let (year, month, day) = parse_day(today)?;
    let (year, month, day) = civil_from_days(days_from_civil(year, month, day).saturating_sub(1));
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_pins_start_and_floors_deletes() {
        let mut log = SessionLog::new();
        log.record(1000, "2026-10-01");
        assert_eq!(log.words_today("2026-10-01"), 0);
        log.record(1312, "2026-10-01");
        assert_eq!(log.words_today("2026-10-01"), 312);
        log.record(900, "2026-10-01");
        assert_eq!(log.words_today("2026-10-01"), 0);
        // A new day starts from its own first observation.
        log.record(900, "2026-10-02");
        assert_eq!(log.words_today("2026-10-02"), 0);
        log.record(950, "2026-10-02");
        assert_eq!(log.words_today("2026-10-02"), 50);
        // Malformed dates are ignored, never crash.
        log.record(9999, "yesterday");
        assert_eq!(log.words_today("yesterday"), 0);
    }

    #[test]
    fn streak_counts_back_over_month_boundary() {
        let mut log = SessionLog::new();
        for (total, day) in [
            (100, "2026-09-29"),
            (200, "2026-09-30"),
            (300, "2026-10-01"),
        ] {
            log.record(total, day);
            log.record(total.saturating_add(10), day);
        }
        assert_eq!(log.streak_days("2026-10-01"), 3);
        // Unwritten today keeps yesterday's streak alive.
        assert_eq!(log.streak_days("2026-10-02"), 3);
        // A zero day in the middle breaks it.
        let mut broken = SessionLog::new();
        broken.record(100, "2026-10-01");
        broken.record(120, "2026-10-01");
        broken.record(120, "2026-10-02");
        broken.record(130, "2026-10-03");
        broken.record(150, "2026-10-03");
        assert_eq!(broken.streak_days("2026-10-03"), 1);
    }

    #[test]
    fn goal_progress_clamps_without_clock() {
        let mut log = SessionLog::new();
        assert!(log.progress("2026-10-04").abs() < f64::EPSILON);
        log.set_goal(500);
        assert_eq!(log.goal(), 500);
        log.record(1000, "2026-10-04");
        log.record(1310, "2026-10-04");
        assert!((log.progress("2026-10-04") - 0.62).abs() < f64::EPSILON);
        log.record(2000, "2026-10-04");
        assert!((log.progress("2026-10-04") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn prev_day_steps_over_year_boundary() {
        assert_eq!(prev_day("2026-01-01").as_deref(), Some("2025-12-31"));
        assert_eq!(prev_day("2026-03-01").as_deref(), Some("2026-02-28"));
        assert_eq!(prev_day("not-a-day"), None);
    }
}
