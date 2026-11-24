use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::catalog;
use crate::model::*;

const FILE_NAME: &str = "gymbuddy.json";

/// Everything the app persists, stored as one JSON document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Data {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub custom_exercises: Vec<Exercise>,
    #[serde(default)]
    pub routines: Vec<Routine>,
    /// Finished workouts, newest first.
    #[serde(default)]
    pub history: Vec<Workout>,
    #[serde(default)]
    pub active: Option<Workout>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FinishError {
    NoActiveWorkout,
    NothingCompleted,
}

pub struct Store {
    path: Option<PathBuf>,
    pub data: Data,
    builtin: Vec<Exercise>,
    counter: u64,
}

impl Store {
    pub fn in_memory() -> Self {
        Self { path: None, data: Data::default(), builtin: catalog::builtin(), counter: 0 }
    }

    /// Opens (or creates) the store in `dir`. A corrupt file is moved aside rather than lost.
    pub fn open(dir: &Path) -> Self {
        let _ = fs::create_dir_all(dir);
        let path = dir.join(FILE_NAME);
        let data = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
                let _ = fs::rename(&path, dir.join("gymbuddy.corrupt.json"));
                Data::default()
            }),
            Err(_) => Data::default(),
        };
        Self { path: Some(path), data, builtin: catalog::builtin(), counter: 0 }
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&self.data)?)?;
        fs::rename(tmp, path)
    }

    pub fn new_id(&mut self) -> String {
        self.counter += 1;
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        format!("{nanos:x}{:x}", self.counter)
    }

    // ----- exercises -----

    pub fn exercises(&self) -> impl Iterator<Item = &Exercise> {
        self.builtin.iter().chain(self.data.custom_exercises.iter())
    }

    pub fn exercise(&self, id: &str) -> Option<&Exercise> {
        self.exercises().find(|e| e.id == id)
    }

    pub fn exercise_name(&self, id: &str) -> String {
        self.exercise(id).map(|e| e.name.clone()).unwrap_or_else(|| id.to_string())
    }

    pub fn exercise_muscle(&self, id: &str) -> String {
        self.exercise(id).map(|e| e.muscle.clone()).unwrap_or_else(|| "Other".into())
    }

    pub fn find_exercise_by_name(&self, name: &str) -> Option<&Exercise> {
        let name = name.trim();
        self.exercises().find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Adds a custom exercise, or returns the id of an existing one with the same name.
    pub fn add_custom_exercise(&mut self, name: &str, muscle: &str) -> Option<String> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        if let Some(e) = self.find_exercise_by_name(name) {
            return Some(e.id.clone());
        }
        let id = format!("custom_{}", self.new_id());
        self.data.custom_exercises.push(Exercise {
            id: id.clone(),
            name: name.to_string(),
            muscle: if muscle.is_empty() { "Other".into() } else { muscle.to_string() },
            equipment: "Other".into(),
            custom: true,
        });
        Some(id)
    }

    /// Sets from the most recent finished workout that included this exercise.
    pub fn previous_sets(&self, exercise_id: &str) -> &[SetEntry] {
        self.data
            .history
            .iter()
            .flat_map(|w| w.exercises.iter())
            .find(|e| e.exercise_id == exercise_id && !e.sets.is_empty())
            .map(|e| e.sets.as_slice())
            .unwrap_or(&[])
    }

    fn prefilled_sets(&self, exercise_id: &str, template: &[SetEntry]) -> Vec<SetEntry> {
        let prev = self.previous_sets(exercise_id);
        let count = if template.is_empty() { prev.len().max(1) } else { template.len() };
        (0..count)
            .map(|i| {
                let t = template.get(i).cloned().unwrap_or_default();
                if t.weight_kg > 0.0 || t.reps > 0 {
                    return SetEntry { done: false, ..t };
                }
                match prev.get(i).or(prev.last()) {
                    Some(p) => SetEntry { kind: t.kind, weight_kg: p.weight_kg, reps: p.reps, done: false },
                    None => SetEntry { kind: t.kind, ..SetEntry::default() },
                }
            })
            .collect()
    }

    // ----- active workout -----

    pub fn active(&self) -> Option<&Workout> {
        self.data.active.as_ref()
    }

    pub fn active_mut(&mut self) -> Option<&mut Workout> {
        self.data.active.as_mut()
    }

    pub fn start_workout(&mut self, routine_id: Option<&str>, now: i64) {
        let routine = routine_id.and_then(|id| self.data.routines.iter().find(|r| r.id == id)).cloned();
        let name = routine.as_ref().map(|r| r.name.clone()).unwrap_or_else(|| default_name(now));
        let template = routine.map(|r| r.exercises).unwrap_or_default();
        self.start_from_template(name, &template, now);
    }

    /// Starts a new workout that repeats a finished one.
    pub fn repeat_workout(&mut self, workout_id: &str, now: i64) {
        if let Some(w) = self.data.history.iter().find(|w| w.id == workout_id).cloned() {
            self.start_from_template(w.name, &w.exercises, now);
        }
    }

    fn start_from_template(&mut self, name: String, template: &[WorkoutExercise], now: i64) {
        let exercises = template
            .iter()
            .map(|t| WorkoutExercise {
                exercise_id: t.exercise_id.clone(),
                sets: self.prefilled_sets(&t.exercise_id, &t.sets),
                rest_secs: t.rest_secs,
                notes: String::new(),
            })
            .collect();
        let id = self.new_id();
        self.data.active = Some(Workout { id, name, started_at: now, ended_at: None, exercises });
    }

    pub fn add_exercise_to_active(&mut self, exercise_id: &str) {
        let sets = self.prefilled_sets(exercise_id, &[]);
        let rest = self.data.settings.default_rest_secs;
        if let Some(w) = self.data.active.as_mut() {
            let mut e = WorkoutExercise::new(exercise_id, rest);
            e.sets = sets;
            w.exercises.push(e);
        }
    }

    pub fn add_set(&mut self, exercise_index: usize) {
        let Some(id) = self.active().and_then(|w| w.exercises.get(exercise_index)).map(|e| e.exercise_id.clone())
        else {
            return;
        };
        let prev = self.previous_sets(&id).to_vec();
        if let Some(e) = self.active_mut().and_then(|w| w.exercises.get_mut(exercise_index)) {
            let i = e.sets.len();
            let set = match (prev.get(i), e.sets.last()) {
                (Some(p), _) => SetEntry::new(p.weight_kg, p.reps),
                (None, Some(last)) => SetEntry::new(last.weight_kg, last.reps),
                (None, None) => SetEntry::default(),
            };
            e.sets.push(set);
        }
    }

    pub fn finish_workout(&mut self, now: i64) -> Result<Workout, FinishError> {
        let mut w = self.data.active.take().ok_or(FinishError::NoActiveWorkout)?;
        let original = w.clone();
        for e in &mut w.exercises {
            e.sets.retain(|s| s.done);
        }
        w.exercises.retain(|e| !e.sets.is_empty());
        if w.exercises.is_empty() {
            self.data.active = Some(original);
            return Err(FinishError::NothingCompleted);
        }
        w.ended_at = Some(now);
        self.data.history.push(w.clone());
        self.sort_history();
        Ok(w)
    }

    pub fn discard_workout(&mut self) {
        self.data.active = None;
    }

    pub fn sort_history(&mut self) {
        self.data.history.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    }

    pub fn workout(&self, id: &str) -> Option<&Workout> {
        self.data.history.iter().find(|w| w.id == id)
    }

    pub fn delete_workout(&mut self, id: &str) {
        self.data.history.retain(|w| w.id != id);
    }

    // ----- routines -----

    pub fn routine(&self, id: &str) -> Option<&Routine> {
        self.data.routines.iter().find(|r| r.id == id)
    }

    pub fn routine_from_workout(&mut self, workout_id: &str) -> Option<String> {
        let w = self.workout(workout_id)?.clone();
        let id = self.new_id();
        let exercises = w
            .exercises
            .into_iter()
            .map(|mut e| {
                for s in &mut e.sets {
                    s.done = false;
                }
                e
            })
            .collect();
        self.data.routines.push(Routine { id: id.clone(), name: w.name, exercises });
        Some(id)
    }

    pub fn upsert_routine(&mut self, routine: Routine) {
        match self.data.routines.iter_mut().find(|r| r.id == routine.id) {
            Some(existing) => *existing = routine,
            None => self.data.routines.push(routine),
        }
    }

    pub fn delete_routine(&mut self, id: &str) {
        self.data.routines.retain(|r| r.id != id);
    }
}

fn default_name(now: i64) -> String {
    use chrono::{DateTime, Local, Timelike};
    let hour = DateTime::from_timestamp(now, 0).map(|d| d.with_timezone(&Local).hour()).unwrap_or(12);
    match hour {
        5..=11 => "Morning Workout",
        12..=16 => "Afternoon Workout",
        17..=21 => "Evening Workout",
        _ => "Night Workout",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done_workout(store: &mut Store, ex: &str, sets: &[(f64, u32)], at: i64) {
        store.start_workout(None, at);
        store.add_exercise_to_active(ex);
        let w = store.active_mut().unwrap();
        w.exercises[0].sets = sets.iter().map(|&(kg, r)| SetEntry::new(kg, r).done()).collect();
        store.finish_workout(at + 3600).unwrap();
    }

    #[test]
    fn finish_drops_incomplete_sets_and_empty_exercises() {
        let mut s = Store::in_memory();
        s.start_workout(None, 1_000);
        s.add_exercise_to_active("squat");
        s.add_exercise_to_active("bench_press");
        {
            let w = s.active_mut().unwrap();
            w.exercises[0].sets = vec![SetEntry::new(100.0, 5).done(), SetEntry::new(100.0, 5)];
        }
        let w = s.finish_workout(2_000).unwrap();
        assert_eq!(w.exercises.len(), 1);
        assert_eq!(w.exercises[0].sets.len(), 1);
        assert_eq!(w.ended_at, Some(2_000));
        assert!(s.active().is_none());
        assert_eq!(s.data.history.len(), 1);
    }

    #[test]
    fn finish_with_nothing_done_keeps_workout_active() {
        let mut s = Store::in_memory();
        s.start_workout(None, 1_000);
        s.add_exercise_to_active("squat");
        assert_eq!(s.finish_workout(2_000), Err(FinishError::NothingCompleted));
        assert!(s.active().is_some());
        assert_eq!(s.finish_workout(2_000).unwrap_err(), FinishError::NothingCompleted);
    }

    #[test]
    fn new_exercise_is_prefilled_from_previous_session() {
        let mut s = Store::in_memory();
        done_workout(&mut s, "squat", &[(100.0, 5), (110.0, 3)], 1_000);
        done_workout(&mut s, "squat", &[(120.0, 2)], 10_000);
        s.start_workout(None, 20_000);
        s.add_exercise_to_active("squat");
        let sets = &s.active().unwrap().exercises[0].sets;
        assert_eq!(sets.len(), 1, "uses most recent session only");
        assert_eq!((sets[0].weight_kg, sets[0].reps, sets[0].done), (120.0, 2, false));
        s.add_set(0);
        assert_eq!(s.active().unwrap().exercises[0].sets[1].weight_kg, 120.0);
    }

    #[test]
    fn routine_round_trip() {
        let mut s = Store::in_memory();
        done_workout(&mut s, "deadlift", &[(140.0, 5), (150.0, 3)], 1_000);
        let wid = s.data.history[0].id.clone();
        let rid = s.routine_from_workout(&wid).unwrap();
        assert!(s.routine(&rid).unwrap().exercises[0].sets.iter().all(|x| !x.done));
        s.start_workout(Some(&rid), 50_000);
        let w = s.active().unwrap();
        assert_eq!(w.exercises[0].sets.len(), 2);
        assert_eq!(w.exercises[0].sets[1].weight_kg, 150.0);
    }

    #[test]
    fn save_and_reload() {
        let dir = std::env::temp_dir().join(format!("gymbuddy-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        {
            let mut s = Store::open(&dir);
            done_workout(&mut s, "bench_press", &[(80.0, 8)], 1_000);
            s.add_custom_exercise("Zercher Squat", "Quads").unwrap();
            s.save().unwrap();
        }
        let s = Store::open(&dir);
        assert_eq!(s.data.history.len(), 1);
        assert!(s.find_exercise_by_name("zercher squat").is_some());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn custom_exercise_dedupes_by_name() {
        let mut s = Store::in_memory();
        let a = s.add_custom_exercise("My Move", "Core").unwrap();
        let b = s.add_custom_exercise(" my move ", "Core").unwrap();
        assert_eq!(a, b);
        assert_eq!(s.add_custom_exercise("bench press (barbell)", "").unwrap(), "bench_press");
        assert!(s.add_custom_exercise("  ", "").is_none());
    }
}
