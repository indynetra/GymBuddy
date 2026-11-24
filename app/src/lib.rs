//! GymBuddy UI: binds the Slint interface to `gymbuddy-core`. Runs on desktop and Android.

use std::cell::RefCell;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gymbuddy_core::{catalog, format as fmt, hevy, stats, FinishError, Routine, SetEntry, SetKind, Store, Unit, Workout, WorkoutExercise};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

slint::include_modules!();

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn ss(s: impl AsRef<str>) -> SharedString {
    s.as_ref().into()
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn kv(key: &str, value: impl ToString) -> KV {
    KV { key: ss(key), value: ss(value.to_string()) }
}

/// Updates `m` in place so repeated UI items (and their focus/scroll state) survive.
fn sync<T: Clone + PartialEq + 'static>(m: &VecModel<T>, items: Vec<T>) {
    for (i, item) in items.iter().enumerate() {
        if i < m.row_count() {
            if m.row_data(i).as_ref() != Some(item) {
                m.set_row_data(i, item.clone());
            }
        } else {
            m.push(item.clone());
        }
    }
    while m.row_count() > items.len() {
        m.remove(m.row_count() - 1);
    }
}

fn idx(i: i32) -> usize {
    i.max(0) as usize
}

#[derive(Clone, Copy, PartialEq)]
enum PickerTarget {
    Active,
    Routine,
}

enum Confirm {
    DiscardWorkout,
    DeleteWorkout,
    DeleteRoutine,
}

struct App {
    store: Store,
    import_dirs: Vec<PathBuf>,
    dirty: bool,
    active_model: Rc<VecModel<ExerciseBlock>>,
    set_models: Vec<Rc<VecModel<SetRow>>>,
    picker_target: PickerTarget,
    picker_query: String,
    picker_muscle: String,
    library_query: String,
    detail_id: Option<String>,
    last_finished: Option<String>,
    routine_draft: Option<Routine>,
    rest_end_ms: Option<i64>,
    rest_total_ms: i64,
    toast_until: Option<i64>,
    confirm: Option<Confirm>,
}

impl App {
    fn unit(&self) -> Unit {
        self.store.data.settings.unit
    }
}

// ---------------------------------------------------------------- refresh

fn set_rows(store: &Store, ex: &WorkoutExercise, unit: Unit) -> Vec<SetRow> {
    let prev = store.previous_sets(&ex.exercise_id);
    let mut n = 0;
    ex.sets
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let label = match s.kind {
                SetKind::Warmup => "W".to_string(),
                SetKind::Drop => "D".to_string(),
                SetKind::Failure => "F".to_string(),
                SetKind::Normal => {
                    n += 1;
                    n.to_string()
                }
            };
            SetRow {
                label: ss(label),
                kind: s.kind.code(),
                weight: ss(fmt::weight_input(s.weight_kg, unit)),
                reps: if s.reps == 0 { SharedString::new() } else { ss(s.reps.to_string()) },
                done: s.done,
                previous: ss(prev.get(i).map(|p| fmt::set(p.weight_kg, p.reps, unit)).unwrap_or_else(|| "—".into())),
            }
        })
        .collect()
}

fn refresh_home(ui: &AppWindow, a: &App) {
    let routines = a
        .store
        .data
        .routines
        .iter()
        .map(|r| RoutineItem {
            id: ss(&r.id),
            name: ss(&r.name),
            summary: ss(if r.exercises.is_empty() {
                "No exercises".to_string()
            } else {
                r.exercises.iter().map(|e| a.store.exercise_name(&e.exercise_id)).collect::<Vec<_>>().join(", ")
            }),
        })
        .collect();
    ui.set_routines(model(routines));
    ui.set_has_active(a.store.active().is_some());
    ui.set_unit_label(ss(a.unit().label()));
}

fn refresh_active(ui: &AppWindow, a: &mut App) {
    let unit = a.unit();
    let Some(w) = a.store.active() else {
        a.active_model.set_vec(Vec::new());
        a.set_models.clear();
        ui.set_has_active(false);
        return;
    };
    ui.set_has_active(true);
    ui.set_active_name(ss(&w.name));
    let data: Vec<(String, String, u32, Vec<SetRow>)> = w
        .exercises
        .iter()
        .map(|e| {
            (a.store.exercise_name(&e.exercise_id), a.store.exercise_muscle(&e.exercise_id), e.rest_secs, set_rows(&a.store, e, unit))
        })
        .collect();
    while a.set_models.len() < data.len() {
        a.set_models.push(Rc::new(VecModel::default()));
    }
    a.set_models.truncate(data.len());
    let blocks = data
        .into_iter()
        .zip(a.set_models.iter())
        .map(|((name, muscle, rest, rows), sm)| {
            sync(sm, rows);
            ExerciseBlock {
                name: ss(name),
                muscle: ss(muscle),
                rest: ss(if rest == 0 { "off".to_string() } else { fmt::clock(rest as i64) }),
                sets: ModelRc::from(sm.clone()),
            }
        })
        .collect();
    sync(&a.active_model, blocks);
    tick(ui, a);
}

fn history_item(store: &Store, w: &Workout, unit: Unit) -> HistoryItem {
    let names: Vec<String> =
        w.exercises.iter().map(|e| format!("{} × {}", e.sets.len(), store.exercise_name(&e.exercise_id))).collect();
    let mut summary = names.iter().take(4).cloned().collect::<Vec<_>>().join("\n");
    if names.len() > 4 {
        summary.push_str(&format!("\n+{} more", names.len() - 4));
    }
    HistoryItem {
        id: ss(&w.id),
        name: ss(&w.name),
        date: ss(fmt::date_time(w.started_at)),
        duration: ss(fmt::duration(w.duration_secs(w.started_at))),
        volume: ss(fmt::volume(w.volume_kg(), unit)),
        sets: ss(fmt::count(w.completed_sets(), "set")),
        summary: ss(summary),
    }
}

fn refresh_history(ui: &AppWindow, a: &App) {
    let items = a.store.data.history.iter().map(|w| history_item(&a.store, w, a.unit())).collect();
    ui.set_history(model(items));
}

fn matches(name: &str, query: &str) -> bool {
    let n = name.to_lowercase();
    query.to_lowercase().split_whitespace().all(|t| n.contains(t))
}

fn exercise_items(a: &App, query: &str, muscle: &str, with_best: bool) -> Vec<ExerciseItem> {
    let mut list: Vec<_> = a
        .store
        .exercises()
        .filter(|e| (muscle.is_empty() || e.muscle == muscle) && matches(&e.name, query))
        .map(|e| {
            let best = if with_best {
                stats::exercise_summary(&a.store.data.history, &e.id)
                    .best_set
                    .map(|s| fmt::set(s.weight_kg, s.reps, a.unit()))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let custom = if e.custom { " · custom" } else { "" };
            ExerciseItem {
                id: ss(&e.id),
                name: ss(&e.name),
                subtitle: ss(format!("{} · {}{}", e.muscle, e.equipment, custom)),
                best: ss(best),
            }
        })
        .collect();
    list.sort_by(|x, y| x.name.to_lowercase().cmp(&y.name.to_lowercase()));
    list
}

fn refresh_library(ui: &AppWindow, a: &App) {
    ui.set_library(model(exercise_items(a, &a.library_query, "", true)));
}

fn refresh_picker(ui: &AppWindow, a: &App) {
    ui.set_picker_muscle(ss(&a.picker_muscle));
    ui.set_picker_items(model(exercise_items(a, &a.picker_query, &a.picker_muscle, false)));
}

fn short_num(v: f32) -> String {
    if v >= 1000.0 {
        format!("{}k", fmt::num(v as f64 / 1000.0, 1))
    } else {
        fmt::num(v as f64, 0)
    }
}

fn refresh_stats(ui: &AppWindow, a: &App) {
    let unit = a.unit();
    let h = &a.store.data.history;
    let t = now();
    let weeks = stats::weekly(h, t, 8);
    ui.set_totals(model(vec![
        kv("Workouts", h.len()),
        kv("This week", weeks.last().map(|w| w.workouts).unwrap_or(0)),
        kv("Week streak", stats::week_streak(h, t)),
    ]));
    let bars: Vec<Bar> = weeks
        .iter()
        .map(|w| {
            let v = unit.from_kg(w.volume_kg) as f32;
            Bar { label: ss(fmt::short_date(w.start)), value: v, text: ss(if v > 0.0 { short_num(v) } else { String::new() }) }
        })
        .collect();
    ui.set_weekly_max(bars.iter().map(|b| b.value).fold(0.0, f32::max));
    ui.set_weekly_bars(model(bars));
    let split = stats::muscle_sets(h, t - 30 * 86_400, |id| a.store.exercise_muscle(id));
    let top = split.first().map(|x| x.1).unwrap_or(1).max(1) as f32;
    ui.set_muscle_split(model(
        split.iter().map(|(m, n)| Share { name: ss(m), value: ss(n.to_string()), fraction: *n as f32 / top }).collect(),
    ));
    ui.set_default_rest(ss(fmt::clock(a.store.data.settings.default_rest_secs as i64)));
    ui.set_unit_label(ss(unit.label()));
}

fn refresh_routine(ui: &AppWindow, a: &App) {
    let Some(r) = &a.routine_draft else { return };
    let blocks = r
        .exercises
        .iter()
        .map(|e| ExerciseBlock {
            name: ss(a.store.exercise_name(&e.exercise_id)),
            muscle: ss(a.store.exercise_muscle(&e.exercise_id)),
            rest: ss(fmt::clock(e.rest_secs as i64)),
            sets: model(vec![SetRow::default(); e.sets.len()]),
        })
        .collect();
    ui.set_routine_exercises(model(blocks));
}

fn refresh_all(ui: &AppWindow, a: &mut App) {
    refresh_home(ui, a);
    refresh_active(ui, a);
    refresh_history(ui, a);
    refresh_library(ui, a);
    refresh_stats(ui, a);
}

fn toast(ui: &AppWindow, a: &mut App, msg: &str) {
    ui.set_toast(ss(msg));
    a.toast_until = Some(now_ms() + 2600);
}

fn tick(ui: &AppWindow, a: &mut App) {
    let t = now();
    if let Some(w) = a.store.active() {
        ui.set_elapsed(ss(fmt::clock(w.duration_secs(t))));
        ui.set_active_summary(ss(format!("{} · {}", fmt::count(w.completed_sets(), "set"), fmt::volume(w.volume_kg(), a.unit()))));
    }
    let ms = now_ms();
    match a.rest_end_ms {
        Some(end) if end > ms => {
            let left = end - ms;
            ui.set_rest_running(true);
            ui.set_rest_left(ss(fmt::clock((left + 999) / 1000)));
            ui.set_rest_fraction((left as f32 / a.rest_total_ms.max(1) as f32).clamp(0.0, 1.0));
        }
        Some(_) => {
            a.rest_end_ms = None;
            ui.set_rest_running(false);
            toast(ui, a, "Rest over. Next set!");
        }
        None => ui.set_rest_running(false),
    }
    if a.toast_until.is_some_and(|until| ms > until) {
        a.toast_until = None;
        ui.set_toast(SharedString::new());
    }
    if a.dirty {
        a.dirty = false;
        if let Err(e) = a.store.save() {
            toast(ui, a, &format!("Could not save: {e}"));
        }
    }
}

// ---------------------------------------------------------------- actions

fn open_exercise(ui: &AppWindow, a: &App, id: &str) {
    let unit = a.unit();
    let h = &a.store.data.history;
    let summary = stats::exercise_summary(h, id);
    let sessions = stats::exercise_sessions(h, id);
    let ex = a.store.exercise(id);
    ui.set_ex_name(ss(ex.map(|e| e.name.as_str()).unwrap_or(id)));
    ui.set_ex_subtitle(ss(ex.map(|e| format!("{} · {}", e.muscle, e.equipment)).unwrap_or_default()));
    ui.set_ex_stats(model(vec![
        kv("Best set", summary.best_set.as_ref().map(|s| fmt::set(s.weight_kg, s.reps, unit)).unwrap_or_else(|| "—".into())),
        kv("Est. 1RM", if summary.best_1rm > 0.0 { fmt::weight(summary.best_1rm, unit) } else { "—".into() }),
        kv("Sessions", summary.sessions),
    ]));
    let bars: Vec<Bar> = sessions
        .iter()
        .take(10)
        .rev()
        .map(|s| {
            let v = unit.from_kg(s.best_1rm);
            Bar { label: ss(fmt::short_date(s.when)), value: v as f32, text: ss(fmt::num(v, 0)) }
        })
        .collect();
    let peak = bars.iter().map(|b| b.value).fold(0.0, f32::max);
    let low = bars.iter().map(|b| b.value).fold(f32::MAX, f32::min);
    // Start the axis below the lowest session so progress is visible.
    ui.set_ex_floor(if bars.len() > 1 && low < f32::MAX { (low - (peak - low).max(peak * 0.05)).max(0.0) } else { 0.0 });
    ui.set_ex_max(peak);
    ui.set_ex_bars(model(bars));
    ui.set_ex_sessions(model(
        sessions
            .iter()
            .take(40)
            .map(|s| {
                let sets: Vec<String> = s.sets.iter().map(|x| fmt::set(x.weight_kg, x.reps, unit)).collect();
                kv(&fmt::short_date(s.when), sets.join(",  "))
            })
            .collect(),
    ));
    ui.set_overlay(ss("exercise"));
}

fn open_history(ui: &AppWindow, a: &mut App, id: &str) {
    let unit = a.unit();
    let Some(w) = a.store.workout(id) else { return };
    ui.set_detail(history_item(&a.store, w, unit));
    let blocks = w
        .exercises
        .iter()
        .map(|e| {
            let mut n = 0;
            let lines = e
                .sets
                .iter()
                .map(|s| {
                    let label = match s.kind {
                        SetKind::Warmup => "W".to_string(),
                        SetKind::Drop => "D".to_string(),
                        SetKind::Failure => "F".to_string(),
                        SetKind::Normal => {
                            n += 1;
                            n.to_string()
                        }
                    };
                    ss(format!("{label}    {}", fmt::set(s.weight_kg, s.reps, unit)))
                })
                .collect();
            DetailBlock { name: ss(a.store.exercise_name(&e.exercise_id)), lines: model(lines) }
        })
        .collect();
    ui.set_detail_blocks(model(blocks));
    a.detail_id = Some(id.to_string());
    ui.set_overlay(ss("history"));
}

fn begin_workout(ui: &AppWindow, a: &mut App, start: impl FnOnce(&mut Store)) {
    if a.store.active().is_some() {
        toast(ui, a, "Finish or discard your current workout first.");
        ui.set_overlay(ss("active"));
        return;
    }
    start(&mut a.store);
    a.rest_end_ms = None;
    a.dirty = true;
    refresh_active(ui, a);
    refresh_home(ui, a);
    ui.set_overlay(ss("active"));
}

fn finish(ui: &AppWindow, a: &mut App) {
    match a.store.finish_workout(now()) {
        Ok(w) => {
            let unit = a.unit();
            a.rest_end_ms = None;
            a.dirty = true;
            let records: Vec<SharedString> = stats::new_records(&a.store.data.history, &w)
                .iter()
                .map(|id| {
                    let best = w
                        .exercises
                        .iter()
                        .filter(|e| &e.exercise_id == id)
                        .flat_map(|e| e.sets.iter())
                        .filter(|s| s.kind.counts())
                        .max_by(|x, y| stats::epley_1rm(x.weight_kg, x.reps).total_cmp(&stats::epley_1rm(y.weight_kg, y.reps)))
                        .cloned()
                        .unwrap_or_default();
                    ss(format!(
                        "{}: {} (est. 1RM {})",
                        a.store.exercise_name(id),
                        fmt::set(best.weight_kg, best.reps, unit),
                        fmt::weight(stats::epley_1rm(best.weight_kg, best.reps), unit)
                    ))
                })
                .collect();
            ui.set_summary_title(ss(&w.name));
            ui.set_summary_stats(model(vec![
                kv("Duration", fmt::duration(w.duration_secs(0))),
                kv("Volume", fmt::volume(w.volume_kg(), unit)),
                kv("Sets", w.completed_sets()),
            ]));
            ui.set_summary_records(model(records));
            a.last_finished = Some(w.id);
            refresh_all(ui, a);
            ui.set_overlay(ss("summary"));
        }
        Err(FinishError::NothingCompleted) => toast(ui, a, "Tick off at least one set before finishing."),
        Err(FinishError::NoActiveWorkout) => ui.set_overlay(SharedString::new()),
    }
}

fn ask(ui: &AppWindow, a: &mut App, what: Confirm, text: &str) {
    a.confirm = Some(what);
    ui.set_confirm_text(ss(text));
}

fn close_overlay(ui: &AppWindow, a: &mut App) {
    match ui.get_overlay().as_str() {
        "picker" => {
            let back = if a.picker_target == PickerTarget::Routine { "routine" } else { "active" };
            ui.set_overlay(ss(back));
        }
        "summary" => {
            ui.set_overlay(SharedString::new());
            ui.set_tab(1);
        }
        "routine" => {
            a.routine_draft = None;
            ui.set_overlay(SharedString::new());
        }
        _ => ui.set_overlay(SharedString::new()),
    }
}

fn import_hevy(ui: &AppWindow, a: &mut App) {
    const NAMES: &[&str] = &["hevy.csv", "workouts.csv", "workout_data.csv"];
    let found = a.import_dirs.iter().flat_map(|d| NAMES.iter().map(move |n| d.join(n))).find(|p| p.is_file());
    let status = match found {
        Some(path) => match File::open(&path).map_err(|e| e.to_string()).and_then(|f| hevy::import_hevy_csv(&mut a.store, f)) {
            Ok(r) => {
                a.dirty = true;
                refresh_all(ui, a);
                let mut s = format!("Imported {} workouts ({} sets) from {}.", r.workouts, r.sets, path.display());
                if r.skipped_duplicates > 0 {
                    s.push_str(&format!(" {} were already here.", r.skipped_duplicates));
                }
                if !r.new_exercises.is_empty() {
                    s.push_str(&format!(" Added {} custom exercises.", r.new_exercises.len()));
                }
                s
            }
            Err(e) => format!("Import failed: {e}"),
        },
        None => format!(
            "In Hevy: Profile → Settings → Export & Import Data → Export workouts. Save the CSV as hevy.csv in:\n{}\nthen tap Import CSV again.",
            a.import_dirs.first().map(|d| d.display().to_string()).unwrap_or_default()
        ),
    };
    ui.set_import_status(ss(status));
}

/// Imports a Hevy CSV into the data directory without starting the UI.
pub fn import_hevy_file(data_dir: &Path, csv: &Path) -> Result<String, String> {
    let mut store = Store::open(data_dir);
    let f = File::open(csv).map_err(|e| format!("{}: {e}", csv.display()))?;
    let r = hevy::import_hevy_csv(&mut store, f)?;
    store.save().map_err(|e| e.to_string())?;
    Ok(format!(
        "Imported {} workouts, {} sets ({} duplicates skipped, {} new custom exercises) into {}",
        r.workouts,
        r.sets,
        r.skipped_duplicates,
        r.new_exercises.len(),
        data_dir.display()
    ))
}

// ---------------------------------------------------------------- wiring

macro_rules! on {
    ($ui:ident, $app:ident, $setter:ident, |$u:ident, $a:ident $(, $arg:ident : $ty:ty)*| $body:block) => {{
        let weak = $ui.as_weak();
        let state = $app.clone();
        $ui.$setter(move |$($arg: $ty),*| {
            let Some($u) = weak.upgrade() else { return Default::default() };
            let Ok(mut guard) = state.try_borrow_mut() else { return Default::default() };
            let $a: &mut App = &mut guard;
            $body
        });
    }};
}

fn with_set(a: &mut App, ei: i32, si: i32, f: impl FnOnce(&mut SetEntry)) -> bool {
    match a.store.active_mut().and_then(|w| w.exercises.get_mut(idx(ei))).and_then(|e| e.sets.get_mut(idx(si))) {
        Some(s) => {
            f(s);
            a.dirty = true;
            true
        }
        None => false,
    }
}

fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim().replace(',', ".");
    if t.is_empty() {
        Some(0.0)
    } else {
        t.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0)
    }
}

pub fn run(data_dir: PathBuf, import_dirs: Vec<PathBuf>) -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let app = Rc::new(RefCell::new(App {
        store: Store::open(&data_dir),
        import_dirs,
        dirty: false,
        active_model: Rc::new(VecModel::default()),
        set_models: Vec::new(),
        picker_target: PickerTarget::Active,
        picker_query: String::new(),
        picker_muscle: String::new(),
        library_query: String::new(),
        detail_id: None,
        last_finished: None,
        routine_draft: None,
        rest_end_ms: None,
        rest_total_ms: 1,
        toast_until: None,
        confirm: None,
    }));
    {
        let mut a = app.borrow_mut();
        ui.set_muscles(model(catalog::MUSCLES.iter().map(ss).collect()));
        ui.set_active_exercises(ModelRc::from(a.active_model.clone()));
        refresh_all(&ui, &mut a);
        if a.store.active().is_some() {
            ui.set_overlay(ss("active"));
        }
    }

    // --- starting and finishing
    on!(ui, app, on_start_empty, |ui, a| { begin_workout(&ui, a, |s| s.start_workout(None, now())); });
    on!(ui, app, on_start_routine, |ui, a, id: SharedString| {
        begin_workout(&ui, a, |s| s.start_workout(Some(id.as_str()), now()));
    });
    on!(ui, app, on_resume, |ui, a| {
        refresh_active(&ui, a);
        ui.set_overlay(ss("active"));
    });
    on!(ui, app, on_finish_workout, |ui, a| { finish(&ui, a); });
    on!(ui, app, on_discard_workout, |ui, a| {
        ask(&ui, a, Confirm::DiscardWorkout, "Discard this workout? Logged sets will be lost.");
    });

    // --- sets
    on!(ui, app, on_add_set, |ui, a, ei: i32| {
        a.store.add_set(idx(ei));
        a.dirty = true;
        refresh_active(&ui, a);
    });
    on!(ui, app, on_remove_set, |ui, a, ei: i32, si: i32| {
        if let Some(e) = a.store.active_mut().and_then(|w| w.exercises.get_mut(idx(ei))) {
            if idx(si) < e.sets.len() {
                e.sets.remove(idx(si));
            }
        }
        a.dirty = true;
        refresh_active(&ui, a);
    });
    on!(ui, app, on_cycle_kind, |ui, a, ei: i32, si: i32| {
        if with_set(a, ei, si, |s| s.kind = s.kind.next()) {
            refresh_active(&ui, a);
        }
    });
    on!(ui, app, on_set_weight, |ui, a, ei: i32, si: i32, text: SharedString| {
        let unit = a.unit();
        if let Some(v) = parse_number(&text) {
            with_set(a, ei, si, |s| s.weight_kg = unit.to_kg(v));
            tick(&ui, a);
        }
    });
    on!(ui, app, on_set_reps, |ui, a, ei: i32, si: i32, text: SharedString| {
        if let Some(v) = parse_number(&text) {
            with_set(a, ei, si, |s| s.reps = v.round().min(10_000.0) as u32);
            tick(&ui, a);
        }
    });
    on!(ui, app, on_copy_previous, |ui, a, ei: i32, si: i32| {
        let id = a.store.active().and_then(|w| w.exercises.get(idx(ei))).map(|e| e.exercise_id.clone());
        let prev = id.and_then(|id| a.store.previous_sets(&id).get(idx(si)).cloned());
        if let Some(p) = prev {
            with_set(a, ei, si, |s| {
                s.weight_kg = p.weight_kg;
                s.reps = p.reps;
            });
            refresh_active(&ui, a);
        }
    });
    on!(ui, app, on_toggle_done, |ui, a, ei: i32, si: i32| {
        let id = a.store.active().and_then(|w| w.exercises.get(idx(ei))).map(|e| e.exercise_id.clone());
        let prev = id.and_then(|id| a.store.previous_sets(&id).get(idx(si)).cloned());
        let rest = a.store.active().and_then(|w| w.exercises.get(idx(ei))).map(|e| e.rest_secs).unwrap_or(0);
        let mut now_done = false;
        with_set(a, ei, si, |s| {
            if !s.done && s.weight_kg == 0.0 && s.reps == 0 {
                if let Some(p) = &prev {
                    s.weight_kg = p.weight_kg;
                    s.reps = p.reps;
                }
            }
            s.done = !s.done;
            now_done = s.done;
        });
        if now_done && rest > 0 {
            a.rest_total_ms = rest as i64 * 1000;
            a.rest_end_ms = Some(now_ms() + a.rest_total_ms);
        }
        refresh_active(&ui, a);
    });
    on!(ui, app, on_remove_exercise, |ui, a, ei: i32| {
        if let Some(w) = a.store.active_mut() {
            if idx(ei) < w.exercises.len() {
                w.exercises.remove(idx(ei));
            }
        }
        a.dirty = true;
        refresh_active(&ui, a);
    });
    on!(ui, app, on_rest_adjust, |ui, a, ei: i32, delta: i32| {
        if let Some(e) = a.store.active_mut().and_then(|w| w.exercises.get_mut(idx(ei))) {
            e.rest_secs = (e.rest_secs as i32 + delta).clamp(0, 600) as u32;
        }
        a.dirty = true;
        refresh_active(&ui, a);
    });
    on!(ui, app, on_rest_timer_adjust, |ui, a, delta: i32| {
        if let Some(end) = a.rest_end_ms {
            let end = end + delta as i64 * 1000;
            a.rest_total_ms = (a.rest_total_ms + delta as i64 * 1000).max(1000);
            a.rest_end_ms = if end > now_ms() { Some(end) } else { None };
            tick(&ui, a);
        }
    });
    on!(ui, app, on_rest_skip, |ui, a| {
        a.rest_end_ms = None;
        tick(&ui, a);
    });

    // --- exercise picker
    on!(ui, app, on_open_picker, |ui, a, target: SharedString| {
        a.picker_target = if target == "routine" { PickerTarget::Routine } else { PickerTarget::Active };
        a.picker_query.clear();
        a.picker_muscle.clear();
        ui.set_picker_query(SharedString::new());
        refresh_picker(&ui, a);
        ui.set_overlay(ss("picker"));
    });
    on!(ui, app, on_picker_search, |ui, a, q: SharedString| {
        a.picker_query = q.to_string();
        refresh_picker(&ui, a);
    });
    on!(ui, app, on_picker_filter, |ui, a, m: SharedString| {
        a.picker_muscle = m.to_string();
        refresh_picker(&ui, a);
    });
    let pick = {
        let weak = ui.as_weak();
        let state = app.clone();
        move |id: String| {
            let (Some(ui), Ok(mut a)) = (weak.upgrade(), state.try_borrow_mut()) else { return };
            match a.picker_target {
                PickerTarget::Active => {
                    a.store.add_exercise_to_active(&id);
                    a.dirty = true;
                    refresh_active(&ui, &mut a);
                    ui.set_overlay(ss("active"));
                }
                PickerTarget::Routine => {
                    let rest = a.store.data.settings.default_rest_secs;
                    if let Some(r) = a.routine_draft.as_mut() {
                        let mut e = WorkoutExercise::new(id, rest);
                        e.sets = vec![SetEntry::default(); 3];
                        r.exercises.push(e);
                    }
                    refresh_routine(&ui, &a);
                    ui.set_overlay(ss("routine"));
                }
            }
        }
    };
    {
        let pick = pick.clone();
        ui.on_picker_pick(move |id| pick(id.to_string()));
    }
    {
        let state = app.clone();
        let weak = ui.as_weak();
        ui.on_create_custom(move |name| {
            let id = {
                let Ok(mut a) = state.try_borrow_mut() else { return };
                let muscle = a.picker_muscle.clone();
                let id = a.store.add_custom_exercise(&name, &muscle);
                a.dirty = true;
                id
            };
            if let Some(id) = id {
                pick(id);
            }
            if let (Some(ui), Ok(a)) = (weak.upgrade(), state.try_borrow()) {
                refresh_library(&ui, &a);
            }
        });
    }

    // --- history
    on!(ui, app, on_open_history, |ui, a, id: SharedString| { open_history(&ui, a, &id); });
    on!(ui, app, on_delete_history, |ui, a| {
        ask(&ui, a, Confirm::DeleteWorkout, "Delete this workout from your history?");
    });
    on!(ui, app, on_repeat_history, |ui, a| {
        if let Some(id) = a.detail_id.clone() {
            begin_workout(&ui, a, |s| s.repeat_workout(&id, now()));
        }
    });
    on!(ui, app, on_history_to_routine, |ui, a| {
        if let Some(id) = a.detail_id.clone() {
            a.store.routine_from_workout(&id);
            a.dirty = true;
            refresh_home(&ui, a);
            toast(&ui, a, "Saved as a routine.");
        }
    });
    on!(ui, app, on_summary_to_routine, |ui, a| {
        if let Some(id) = a.last_finished.clone() {
            a.store.routine_from_workout(&id);
            a.dirty = true;
            refresh_home(&ui, a);
            toast(&ui, a, "Saved as a routine.");
            ui.set_overlay(SharedString::new());
            ui.set_tab(0);
        }
    });

    // --- library
    on!(ui, app, on_library_search, |ui, a, q: SharedString| {
        a.library_query = q.to_string();
        refresh_library(&ui, a);
    });
    on!(ui, app, on_open_exercise, |ui, a, id: SharedString| { open_exercise(&ui, a, &id); });

    // --- routines
    on!(ui, app, on_new_routine, |ui, a| {
        let id = a.store.new_id();
        a.routine_draft = Some(Routine { id, name: String::new(), exercises: Vec::new() });
        ui.set_routine_name(SharedString::new());
        ui.set_routine_is_new(true);
        refresh_routine(&ui, a);
        ui.set_overlay(ss("routine"));
    });
    on!(ui, app, on_edit_routine, |ui, a, id: SharedString| {
        if let Some(r) = a.store.routine(&id).cloned() {
            ui.set_routine_name(ss(&r.name));
            a.routine_draft = Some(r);
            ui.set_routine_is_new(false);
            refresh_routine(&ui, a);
            ui.set_overlay(ss("routine"));
        }
    });
    on!(ui, app, on_routine_sets, |ui, a, ei: i32, delta: i32| {
        if let Some(e) = a.routine_draft.as_mut().and_then(|r| r.exercises.get_mut(idx(ei))) {
            let n = (e.sets.len() as i32 + delta).clamp(1, 12) as usize;
            e.sets.resize(n, e.sets.last().cloned().unwrap_or_default());
        }
        refresh_routine(&ui, a);
    });
    on!(ui, app, on_routine_remove, |ui, a, ei: i32| {
        if let Some(r) = a.routine_draft.as_mut() {
            if idx(ei) < r.exercises.len() {
                r.exercises.remove(idx(ei));
            }
        }
        refresh_routine(&ui, a);
    });
    on!(ui, app, on_save_routine, |ui, a| {
        let name = ui.get_routine_name().trim().to_string();
        let Some(mut r) = a.routine_draft.clone() else { return };
        if name.is_empty() {
            toast(&ui, a, "Give the routine a name.");
            return;
        }
        if r.exercises.is_empty() {
            toast(&ui, a, "Add at least one exercise.");
            return;
        }
        r.name = name;
        a.store.upsert_routine(r);
        a.routine_draft = None;
        a.dirty = true;
        refresh_home(&ui, a);
        ui.set_overlay(SharedString::new());
        ui.set_tab(0);
        toast(&ui, a, "Routine saved.");
    });
    on!(ui, app, on_delete_routine, |ui, a| {
        ask(&ui, a, Confirm::DeleteRoutine, "Delete this routine? Your workout history is kept.");
    });

    // --- settings
    on!(ui, app, on_toggle_unit, |ui, a| {
        a.store.data.settings.unit = a.unit().toggle();
        a.dirty = true;
        refresh_all(&ui, a);
    });
    on!(ui, app, on_default_rest_adjust, |ui, a, delta: i32| {
        let s = &mut a.store.data.settings;
        s.default_rest_secs = (s.default_rest_secs as i32 + delta).clamp(0, 600) as u32;
        a.dirty = true;
        refresh_stats(&ui, a);
    });
    on!(ui, app, on_import_hevy, |ui, a| { import_hevy(&ui, a); });

    // --- navigation and dialogs
    on!(ui, app, on_close_overlay, |ui, a| { close_overlay(&ui, a); });
    on!(ui, app, on_confirm_cancel, |ui, a| {
        a.confirm = None;
        ui.set_confirm_text(SharedString::new());
    });
    on!(ui, app, on_confirm_accept, |ui, a| {
        ui.set_confirm_text(SharedString::new());
        match a.confirm.take() {
            Some(Confirm::DiscardWorkout) => {
                a.store.discard_workout();
                a.rest_end_ms = None;
                ui.set_overlay(SharedString::new());
                toast(&ui, a, "Workout discarded.");
            }
            Some(Confirm::DeleteWorkout) => {
                if let Some(id) = a.detail_id.take() {
                    a.store.delete_workout(&id);
                }
                ui.set_overlay(SharedString::new());
            }
            Some(Confirm::DeleteRoutine) => {
                if let Some(r) = a.routine_draft.take() {
                    a.store.delete_routine(&r.id);
                }
                ui.set_overlay(SharedString::new());
            }
            None => {}
        }
        a.dirty = true;
        refresh_all(&ui, a);
    });
    on!(ui, app, on_back_pressed, |ui, a| {
        if a.confirm.is_some() {
            a.confirm = None;
            ui.set_confirm_text(SharedString::new());
            return true;
        }
        if !ui.get_overlay().is_empty() {
            close_overlay(&ui, a);
            return true;
        }
        if ui.get_tab() != 0 {
            ui.set_tab(0);
            return true;
        }
        false
    });

    let timer = Timer::default();
    {
        let weak = ui.as_weak();
        let state = app.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(500), move || {
            if let (Some(ui), Ok(mut a)) = (weak.upgrade(), state.try_borrow_mut()) {
                tick(&ui, &mut a);
            }
        });
    }

    ui.run()?;
    let _ = app.borrow().store.save();
    Ok(())
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android_app: slint::android::AndroidApp) {
    let data = android_app.internal_data_path().unwrap_or_else(|| PathBuf::from("."));
    let mut import_dirs = vec![data.clone()];
    if let Some(ext) = android_app.external_data_path() {
        let _ = std::fs::create_dir_all(&ext);
        import_dirs.insert(0, ext);
    }
    slint::android::init(android_app).expect("Slint Android init failed");
    run(data, import_dirs).expect("GymBuddy failed");
}
