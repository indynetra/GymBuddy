use serde::{Deserialize, Serialize};

pub const DEFAULT_REST_SECS: u32 = 90;
pub const LB_PER_KG: f64 = 2.204_622_621_8;

fn default_rest() -> u32 {
    DEFAULT_REST_SECS
}

/// Kind of set. Warm-ups are logged but excluded from volume and records.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetKind {
    #[default]
    Normal,
    Warmup,
    Drop,
    Failure,
}

impl SetKind {
    pub fn next(self) -> Self {
        match self {
            SetKind::Normal => SetKind::Warmup,
            SetKind::Warmup => SetKind::Drop,
            SetKind::Drop => SetKind::Failure,
            SetKind::Failure => SetKind::Normal,
        }
    }

    /// Whether this set counts towards volume and personal records.
    pub fn counts(self) -> bool {
        self != SetKind::Warmup
    }

    pub fn code(self) -> i32 {
        self as i32
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SetEntry {
    #[serde(default)]
    pub kind: SetKind,
    #[serde(default)]
    pub weight_kg: f64,
    #[serde(default)]
    pub reps: u32,
    #[serde(default)]
    pub done: bool,
}

impl SetEntry {
    pub fn new(weight_kg: f64, reps: u32) -> Self {
        Self { kind: SetKind::Normal, weight_kg, reps, done: false }
    }

    pub fn done(mut self) -> Self {
        self.done = true;
        self
    }

    pub fn volume(&self) -> f64 {
        if self.kind.counts() {
            self.weight_kg * self.reps as f64
        } else {
            0.0
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkoutExercise {
    pub exercise_id: String,
    #[serde(default)]
    pub sets: Vec<SetEntry>,
    #[serde(default = "default_rest")]
    pub rest_secs: u32,
    #[serde(default)]
    pub notes: String,
}

impl WorkoutExercise {
    pub fn new(exercise_id: impl Into<String>, rest_secs: u32) -> Self {
        Self { exercise_id: exercise_id.into(), sets: Vec::new(), rest_secs, notes: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workout {
    pub id: String,
    pub name: String,
    pub started_at: i64,
    #[serde(default)]
    pub ended_at: Option<i64>,
    #[serde(default)]
    pub exercises: Vec<WorkoutExercise>,
}

impl Workout {
    pub fn duration_secs(&self, now: i64) -> i64 {
        (self.ended_at.unwrap_or(now) - self.started_at).max(0)
    }

    pub fn done_sets(&self) -> impl Iterator<Item = &SetEntry> {
        self.exercises.iter().flat_map(|e| e.sets.iter()).filter(|s| s.done)
    }

    pub fn completed_sets(&self) -> usize {
        self.done_sets().count()
    }

    pub fn volume_kg(&self) -> f64 {
        self.done_sets().map(SetEntry::volume).sum()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub exercises: Vec<WorkoutExercise>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    #[default]
    Kg,
    Lb,
}

impl Unit {
    pub fn label(self) -> &'static str {
        match self {
            Unit::Kg => "kg",
            Unit::Lb => "lb",
        }
    }

    pub fn from_kg(self, kg: f64) -> f64 {
        match self {
            Unit::Kg => kg,
            Unit::Lb => kg * LB_PER_KG,
        }
    }

    pub fn to_kg(self, value: f64) -> f64 {
        match self {
            Unit::Kg => value,
            Unit::Lb => value / LB_PER_KG,
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Unit::Kg => Unit::Lb,
            Unit::Lb => Unit::Kg,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub unit: Unit,
    #[serde(default = "default_rest")]
    pub default_rest_secs: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { unit: Unit::Kg, default_rest_secs: DEFAULT_REST_SECS }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Exercise {
    pub id: String,
    pub name: String,
    pub muscle: String,
    pub equipment: String,
    #[serde(default)]
    pub custom: bool,
}
