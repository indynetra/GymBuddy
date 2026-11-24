use std::collections::HashMap;

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone};

use crate::model::{SetEntry, Workout};

const DAY: i64 = 86_400;

/// Estimated one-rep max (Epley).
pub fn epley_1rm(weight_kg: f64, reps: u32) -> f64 {
    match reps {
        0 => 0.0,
        1 => weight_kg,
        r => weight_kg * (1.0 + r as f64 / 30.0),
    }
}

fn counted_sets<'a>(w: &'a Workout, exercise_id: &'a str) -> impl Iterator<Item = &'a SetEntry> + 'a {
    w.exercises
        .iter()
        .filter(move |e| e.exercise_id == exercise_id)
        .flat_map(|e| e.sets.iter())
        .filter(|s| s.done && s.kind.counts())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExerciseSummary {
    pub sessions: usize,
    pub total_sets: usize,
    pub total_volume: f64,
    pub best_weight: f64,
    pub best_1rm: f64,
    pub best_set: Option<SetEntry>,
}

pub fn exercise_summary(history: &[Workout], exercise_id: &str) -> ExerciseSummary {
    let mut out = ExerciseSummary::default();
    for w in history {
        let mut any = false;
        for s in counted_sets(w, exercise_id) {
            any = true;
            out.total_sets += 1;
            out.total_volume += s.volume();
            out.best_weight = out.best_weight.max(s.weight_kg);
            let e = epley_1rm(s.weight_kg, s.reps);
            if e > out.best_1rm {
                out.best_1rm = e;
                out.best_set = Some(s.clone());
            }
        }
        if any {
            out.sessions += 1;
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionPoint {
    pub when: i64,
    pub best_1rm: f64,
    pub top_set: SetEntry,
    pub sets: Vec<SetEntry>,
}

/// One entry per workout that included the exercise, in the order of `history` (newest first).
pub fn exercise_sessions(history: &[Workout], exercise_id: &str) -> Vec<SessionPoint> {
    history
        .iter()
        .filter_map(|w| {
            let sets: Vec<SetEntry> = counted_sets(w, exercise_id).cloned().collect();
            let top = sets
                .iter()
                .max_by(|a, b| epley_1rm(a.weight_kg, a.reps).total_cmp(&epley_1rm(b.weight_kg, b.reps)))?
                .clone();
            Some(SessionPoint {
                when: w.started_at,
                best_1rm: epley_1rm(top.weight_kg, top.reps),
                top_set: top,
                sets,
            })
        })
        .collect()
}

/// Exercise ids in `workout` whose estimated 1RM beats every earlier session in `history`.
pub fn new_records(history: &[Workout], workout: &Workout) -> Vec<String> {
    let mut out = Vec::new();
    for e in &workout.exercises {
        if out.contains(&e.exercise_id) {
            continue;
        }
        let best_now = counted_sets(workout, &e.exercise_id)
            .map(|s| epley_1rm(s.weight_kg, s.reps))
            .fold(0.0, f64::max);
        let best_before = history
            .iter()
            .filter(|w| w.id != workout.id && w.started_at < workout.started_at)
            .flat_map(|w| counted_sets(w, &e.exercise_id))
            .map(|s| epley_1rm(s.weight_kg, s.reps))
            .fold(0.0, f64::max);
        if best_before > 0.0 && best_now > best_before {
            out.push(e.exercise_id.clone());
        }
    }
    out
}

/// Unix time of the most recent local Monday 00:00 at or before `ts`.
pub fn week_start(ts: i64) -> i64 {
    let Some(dt) = DateTime::from_timestamp(ts, 0) else { return ts };
    let local = dt.with_timezone(&Local);
    let monday = local.date_naive() - Duration::days(local.weekday().num_days_from_monday() as i64);
    Local
        .from_local_datetime(&monday.and_time(NaiveTime::MIN))
        .earliest()
        .map(|d| d.timestamp())
        .unwrap_or(ts)
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeekBucket {
    pub start: i64,
    pub volume_kg: f64,
    pub workouts: usize,
}

/// `weeks` buckets ending with the current week, oldest first.
pub fn weekly(history: &[Workout], now: i64, weeks: usize) -> Vec<WeekBucket> {
    let current = week_start(now);
    let mut buckets: Vec<WeekBucket> = (0..weeks)
        .rev()
        .map(|i| WeekBucket {
            // step back from mid-week so DST changes never skip a week
            start: week_start(current + 3 * DAY - i as i64 * 7 * DAY),
            volume_kg: 0.0,
            workouts: 0,
        })
        .collect();
    for w in history {
        let ws = week_start(w.started_at);
        if let Some(b) = buckets.iter_mut().find(|b| b.start == ws) {
            b.volume_kg += w.volume_kg();
            b.workouts += 1;
        }
    }
    buckets
}

/// Consecutive weeks with at least one workout. The current week may still be empty.
pub fn week_streak(history: &[Workout], now: i64) -> usize {
    let buckets = weekly(history, now, 520);
    let mut streak = 0;
    for (i, b) in buckets.iter().rev().enumerate() {
        if b.workouts > 0 {
            streak += 1;
        } else if i > 0 {
            break;
        }
    }
    streak
}

/// Completed working sets per muscle group since `since`, largest first.
pub fn muscle_sets(history: &[Workout], since: i64, muscle_of: impl Fn(&str) -> String) -> Vec<(String, usize)> {
    let mut map: HashMap<String, usize> = HashMap::new();
    for w in history.iter().filter(|w| w.started_at >= since) {
        for e in &w.exercises {
            let n = e.sets.iter().filter(|s| s.done && s.kind.counts()).count();
            if n > 0 {
                *map.entry(muscle_of(&e.exercise_id)).or_default() += n;
            }
        }
    }
    let mut v: Vec<_> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SetKind, WorkoutExercise};

    fn w(id: &str, at: i64, ex: &str, sets: &[(f64, u32)]) -> Workout {
        Workout {
            id: id.into(),
            name: "W".into(),
            started_at: at,
            ended_at: Some(at + 3600),
            exercises: vec![WorkoutExercise {
                exercise_id: ex.into(),
                sets: sets.iter().map(|&(kg, r)| SetEntry::new(kg, r).done()).collect(),
                rest_secs: 90,
                notes: String::new(),
            }],
        }
    }

    #[test]
    fn epley() {
        assert_eq!(epley_1rm(100.0, 0), 0.0);
        assert_eq!(epley_1rm(100.0, 1), 100.0);
        assert!((epley_1rm(100.0, 10) - 133.333).abs() < 0.01);
    }

    #[test]
    fn warmups_excluded_from_volume_and_records() {
        let mut a = w("a", 0, "squat", &[(60.0, 5), (100.0, 5)]);
        a.exercises[0].sets[0].kind = SetKind::Warmup;
        assert_eq!(a.volume_kg(), 500.0);
        let s = exercise_summary(&[a], "squat");
        assert_eq!(s.total_sets, 1);
        assert_eq!(s.best_weight, 100.0);
    }

    #[test]
    fn summary_and_sessions() {
        let h = vec![w("b", 2 * DAY, "squat", &[(110.0, 3)]), w("a", 0, "squat", &[(100.0, 5), (90.0, 8)])];
        let s = exercise_summary(&h, "squat");
        assert_eq!(s.sessions, 2);
        assert_eq!(s.total_sets, 3);
        assert_eq!(s.best_weight, 110.0);
        assert_eq!(s.best_set.unwrap().weight_kg, 110.0); // 121 > 116.7
        let sessions = exercise_sessions(&h, "squat");
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[1].top_set.weight_kg, 100.0);
        assert!(exercise_sessions(&h, "bench_press").is_empty());
    }

    #[test]
    fn records_need_a_previous_session() {
        let first = w("a", 0, "squat", &[(100.0, 5)]);
        assert!(new_records(&[first.clone()], &first).is_empty());
        let better = w("b", DAY, "squat", &[(105.0, 5)]);
        let worse = w("c", 2 * DAY, "squat", &[(90.0, 5)]);
        let h = vec![worse.clone(), better.clone(), first];
        assert_eq!(new_records(&h, &better), vec!["squat".to_string()]);
        assert!(new_records(&h, &worse).is_empty());
    }

    #[test]
    fn weekly_buckets_and_streak() {
        let now = 1_764_000_000; // Nov 2025
        let h = vec![
            w("a", now, "squat", &[(100.0, 5)]),
            w("b", now - 7 * DAY, "squat", &[(100.0, 5)]),
            w("c", now - 14 * DAY, "squat", &[(100.0, 5)]),
            w("d", now - 35 * DAY, "squat", &[(100.0, 5)]),
        ];
        let b = weekly(&h, now, 8);
        assert_eq!(b.len(), 8);
        assert_eq!(b[7].workouts, 1);
        assert_eq!(b[7].volume_kg, 500.0);
        assert!(b.windows(2).all(|p| p[0].start < p[1].start));
        assert_eq!(b.iter().map(|x| x.workouts).sum::<usize>(), 4);
        assert_eq!(week_streak(&h, now), 3);
        // Current week empty does not break the streak.
        assert_eq!(week_streak(&h[1..], now), 2);
        assert_eq!(week_streak(&[], now), 0);
    }

    #[test]
    fn week_start_is_monday_midnight() {
        let ts = 1_764_000_000;
        let ws = week_start(ts);
        let d = DateTime::from_timestamp(ws, 0).unwrap().with_timezone(&Local);
        assert_eq!(d.weekday(), chrono::Weekday::Mon);
        assert!(ws <= ts && ts - ws < 7 * DAY + 3600);
    }

    #[test]
    fn muscle_split() {
        let h = vec![w("a", 10, "squat", &[(1.0, 1), (1.0, 1)]), w("b", 20, "bench", &[(1.0, 1)])];
        let m = muscle_sets(&h, 0, |id| if id == "squat" { "Quads".into() } else { "Chest".into() });
        assert_eq!(m, vec![("Quads".into(), 2), ("Chest".into(), 1)]);
        assert_eq!(muscle_sets(&h, 15, |_| "X".into()), vec![("X".into(), 1)]);
    }
}
