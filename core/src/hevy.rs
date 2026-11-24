//! Import of the CSV file produced by Hevy's "Export workout data".

use std::collections::HashMap;
use std::io::Read;

use chrono::{Local, NaiveDateTime, TimeZone};

use crate::model::*;
use crate::store::Store;

#[derive(Debug, Default, PartialEq)]
pub struct ImportReport {
    pub workouts: usize,
    pub sets: usize,
    pub skipped_duplicates: usize,
    pub new_exercises: Vec<String>,
}

fn parse_time(s: &str) -> Option<i64> {
    const FORMATS: &[&str] = &[
        "%d %b %Y, %H:%M",
        "%d %b %Y, %H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S",
    ];
    let s = s.trim();
    FORMATS.iter().find_map(|f| NaiveDateTime::parse_from_str(s, f).ok()).and_then(|n| {
        Local.from_local_datetime(&n).earliest().map(|d| d.timestamp())
    })
}

fn set_kind(s: &str) -> SetKind {
    match s.trim().to_ascii_lowercase().as_str() {
        "warmup" | "warm_up" | "warm-up" => SetKind::Warmup,
        "dropset" | "drop_set" | "drop" => SetKind::Drop,
        "failure" => SetKind::Failure,
        _ => SetKind::Normal,
    }
}

pub fn import_hevy_csv(store: &mut Store, input: impl Read) -> Result<ImportReport, String> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::All).from_reader(input);
    let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
    let col = |name: &str| headers.iter().position(|h| h.eq_ignore_ascii_case(name));
    let (Some(c_title), Some(c_start), Some(c_ex)) = (col("title"), col("start_time"), col("exercise_title")) else {
        return Err("Not a Hevy export: missing title, start_time or exercise_title column".into());
    };
    let c_end = col("end_time");
    let c_type = col("set_type");
    let c_kg = col("weight_kg");
    let c_lb = col("weight_lbs");
    let c_reps = col("reps");
    let c_notes = col("exercise_notes");

    let mut report = ImportReport::default();
    let mut workouts: Vec<Workout> = Vec::new();
    let mut index: HashMap<(String, i64), usize> = HashMap::new();

    for (line, rec) in rdr.records().enumerate() {
        let rec = rec.map_err(|e| format!("row {}: {e}", line + 2))?;
        let get = |c: Option<usize>| c.and_then(|i| rec.get(i)).unwrap_or("");
        let title = get(Some(c_title)).to_string();
        let Some(start) = parse_time(get(Some(c_start))) else {
            return Err(format!("row {}: cannot read start_time '{}'", line + 2, get(Some(c_start))));
        };
        let ex_title = get(Some(c_ex));
        if ex_title.is_empty() {
            continue;
        }
        let ex_id = match store.find_exercise_by_name(ex_title) {
            Some(e) => e.id.clone(),
            None => {
                report.new_exercises.push(ex_title.to_string());
                store.add_custom_exercise(ex_title, "Other").unwrap_or_default()
            }
        };

        let wi = *index.entry((title.clone(), start)).or_insert_with(|| {
            workouts.push(Workout {
                id: String::new(),
                name: if title.is_empty() { "Imported Workout".into() } else { title.clone() },
                started_at: start,
                ended_at: parse_time(get(c_end)).or(Some(start)),
                exercises: Vec::new(),
            });
            workouts.len() - 1
        });
        let w = &mut workouts[wi];
        if w.exercises.last().map(|e| e.exercise_id != ex_id).unwrap_or(true) {
            let mut e = WorkoutExercise::new(ex_id, store.data.settings.default_rest_secs);
            e.notes = get(c_notes).to_string();
            w.exercises.push(e);
        }
        let weight_kg = match get(c_kg).parse::<f64>() {
            Ok(kg) => kg,
            Err(_) => get(c_lb).parse::<f64>().map(|lb| lb / LB_PER_KG).unwrap_or(0.0),
        };
        let reps = get(c_reps).parse::<f64>().map(|r| r.max(0.0).round() as u32).unwrap_or(0);
        w.exercises.last_mut().unwrap().sets.push(SetEntry { kind: set_kind(get(c_type)), weight_kg, reps, done: true });
    }

    for mut w in workouts {
        let dup = store.data.history.iter().any(|h| h.started_at == w.started_at && h.name == w.name);
        if dup {
            report.skipped_duplicates += 1;
            continue;
        }
        w.id = store.new_id();
        report.sets += w.completed_sets();
        report.workouts += 1;
        store.data.history.push(w);
    }
    store.sort_history();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#""title","start_time","end_time","description","exercise_title","superset_id","exercise_notes","set_index","set_type","weight_kg","reps","distance_km","duration_seconds","rpe"
"Push Day","18 Nov 2025, 07:32","18 Nov 2025, 08:40","","Bench Press (Barbell)",,"",0,"warmup",40,10,,,
"Push Day","18 Nov 2025, 07:32","18 Nov 2025, 08:40","","Bench Press (Barbell)",,"",1,"normal",80,8,,,
"Push Day","18 Nov 2025, 07:32","18 Nov 2025, 08:40","","Bench Press (Barbell)",,"",2,"failure",80,6,,,8.5
"Push Day","18 Nov 2025, 07:32","18 Nov 2025, 08:40","","Cable Lateral Raise Super Custom",,"",0,"dropset",10,12,,,
"Legs","20 Nov 2025, 18:00","20 Nov 2025, 19:05","","Squat (Barbell)",,"",0,"normal",100,5,,,
"Legs","20 Nov 2025, 18:00","20 Nov 2025, 19:05","","Plank",,"",0,"normal",,,,60,
"#;

    #[test]
    fn imports_sample_export() {
        let mut s = Store::in_memory();
        let r = import_hevy_csv(&mut s, SAMPLE.as_bytes()).unwrap();
        assert_eq!(r.workouts, 2);
        assert_eq!(r.sets, 6);
        assert_eq!(r.new_exercises, vec!["Cable Lateral Raise Super Custom".to_string()]);
        let legs = &s.data.history[0];
        assert_eq!(legs.name, "Legs");
        assert_eq!(legs.duration_secs(0), 65 * 60);
        let push = &s.data.history[1];
        assert_eq!(push.exercises.len(), 2);
        assert_eq!(push.exercises[0].exercise_id, "bench_press");
        assert_eq!(push.exercises[0].sets[0].kind, SetKind::Warmup);
        assert_eq!(push.exercises[0].sets[2].kind, SetKind::Failure);
        assert_eq!(push.exercises[1].sets[0].kind, SetKind::Drop);
        assert_eq!(push.volume_kg(), 80.0 * 8.0 + 80.0 * 6.0 + 120.0);

        // Importing again does not duplicate.
        let again = import_hevy_csv(&mut s, SAMPLE.as_bytes()).unwrap();
        assert_eq!(again.workouts, 0);
        assert_eq!(again.skipped_duplicates, 2);
        assert_eq!(s.data.history.len(), 2);
    }

    #[test]
    fn pounds_column_is_converted() {
        let csv = "title,start_time,exercise_title,set_type,weight_lbs,reps\nA,2025-01-02 10:00:00,Deadlift (Barbell),normal,225,5\n";
        let mut s = Store::in_memory();
        import_hevy_csv(&mut s, csv.as_bytes()).unwrap();
        let kg = s.data.history[0].exercises[0].sets[0].weight_kg;
        assert!((kg - 102.058).abs() < 0.01);
    }

    #[test]
    fn rejects_other_csv() {
        let mut s = Store::in_memory();
        assert!(import_hevy_csv(&mut s, "a,b\n1,2\n".as_bytes()).is_err());
    }
}
