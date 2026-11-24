use crate::model::Exercise;

pub const MUSCLES: &[&str] = &[
    "Chest", "Back", "Shoulders", "Biceps", "Triceps", "Quads", "Hamstrings", "Glutes", "Calves",
    "Core", "Other",
];

const BUILTIN: &[(&str, &str, &str, &str)] = &[
    // Chest
    ("bench_press", "Bench Press (Barbell)", "Chest", "Barbell"),
    ("incline_bench_press", "Incline Bench Press (Barbell)", "Chest", "Barbell"),
    ("db_bench_press", "Bench Press (Dumbbell)", "Chest", "Dumbbell"),
    ("incline_db_press", "Incline Bench Press (Dumbbell)", "Chest", "Dumbbell"),
    ("db_fly", "Chest Fly (Dumbbell)", "Chest", "Dumbbell"),
    ("cable_fly", "Cable Fly Crossovers", "Chest", "Cable"),
    ("machine_chest_press", "Chest Press (Machine)", "Chest", "Machine"),
    ("push_up", "Push Up", "Chest", "Bodyweight"),
    ("chest_dip", "Chest Dip", "Chest", "Bodyweight"),
    // Back
    ("deadlift", "Deadlift (Barbell)", "Back", "Barbell"),
    ("barbell_row", "Bent Over Row (Barbell)", "Back", "Barbell"),
    ("db_row", "Dumbbell Row", "Back", "Dumbbell"),
    ("pull_up", "Pull Up", "Back", "Bodyweight"),
    ("chin_up", "Chin Up", "Back", "Bodyweight"),
    ("lat_pulldown", "Lat Pulldown (Cable)", "Back", "Cable"),
    ("seated_cable_row", "Seated Cable Row", "Back", "Cable"),
    ("t_bar_row", "T Bar Row", "Back", "Machine"),
    // Shoulders
    ("overhead_press", "Overhead Press (Barbell)", "Shoulders", "Barbell"),
    ("db_shoulder_press", "Shoulder Press (Dumbbell)", "Shoulders", "Dumbbell"),
    ("arnold_press", "Arnold Press (Dumbbell)", "Shoulders", "Dumbbell"),
    ("lateral_raise", "Lateral Raise (Dumbbell)", "Shoulders", "Dumbbell"),
    ("cable_lateral_raise", "Lateral Raise (Cable)", "Shoulders", "Cable"),
    ("rear_delt_fly", "Rear Delt Reverse Fly (Dumbbell)", "Shoulders", "Dumbbell"),
    ("face_pull", "Face Pull", "Shoulders", "Cable"),
    // Biceps
    ("barbell_curl", "Bicep Curl (Barbell)", "Biceps", "Barbell"),
    ("db_curl", "Bicep Curl (Dumbbell)", "Biceps", "Dumbbell"),
    ("hammer_curl", "Hammer Curl (Dumbbell)", "Biceps", "Dumbbell"),
    ("preacher_curl", "Preacher Curl (Machine)", "Biceps", "Machine"),
    ("cable_curl", "Bicep Curl (Cable)", "Biceps", "Cable"),
    // Triceps
    ("triceps_pushdown", "Triceps Pushdown", "Triceps", "Cable"),
    ("skull_crusher", "Skullcrusher (Barbell)", "Triceps", "Barbell"),
    ("overhead_triceps_ext", "Overhead Triceps Extension (Cable)", "Triceps", "Cable"),
    ("close_grip_bench", "Close Grip Bench Press", "Triceps", "Barbell"),
    ("bench_dip", "Bench Dip", "Triceps", "Bodyweight"),
    // Quads
    ("squat", "Squat (Barbell)", "Quads", "Barbell"),
    ("front_squat", "Front Squat", "Quads", "Barbell"),
    ("leg_press", "Leg Press (Machine)", "Quads", "Machine"),
    ("leg_extension", "Leg Extension (Machine)", "Quads", "Machine"),
    ("hack_squat", "Hack Squat (Machine)", "Quads", "Machine"),
    ("goblet_squat", "Goblet Squat", "Quads", "Dumbbell"),
    ("bulgarian_split_squat", "Bulgarian Split Squat", "Quads", "Dumbbell"),
    ("lunge", "Lunge (Dumbbell)", "Quads", "Dumbbell"),
    // Hamstrings
    ("romanian_deadlift", "Romanian Deadlift (Barbell)", "Hamstrings", "Barbell"),
    ("lying_leg_curl", "Lying Leg Curl (Machine)", "Hamstrings", "Machine"),
    ("seated_leg_curl", "Seated Leg Curl (Machine)", "Hamstrings", "Machine"),
    ("good_morning", "Good Morning (Barbell)", "Hamstrings", "Barbell"),
    // Glutes
    ("hip_thrust", "Hip Thrust (Barbell)", "Glutes", "Barbell"),
    ("glute_bridge", "Glute Bridge", "Glutes", "Bodyweight"),
    ("cable_kickback", "Glute Kickback (Cable)", "Glutes", "Cable"),
    // Calves
    ("standing_calf_raise", "Standing Calf Raise (Machine)", "Calves", "Machine"),
    ("seated_calf_raise", "Seated Calf Raise", "Calves", "Machine"),
    // Core
    ("plank", "Plank", "Core", "Bodyweight"),
    ("crunch", "Crunch", "Core", "Bodyweight"),
    ("hanging_leg_raise", "Hanging Leg Raise", "Core", "Bodyweight"),
    ("cable_crunch", "Cable Crunch", "Core", "Cable"),
    ("ab_wheel", "Ab Wheel", "Core", "Bodyweight"),
];

pub fn builtin() -> Vec<Exercise> {
    BUILTIN
        .iter()
        .map(|(id, name, muscle, equipment)| Exercise {
            id: (*id).into(),
            name: (*name).into(),
            muscle: (*muscle).into(),
            equipment: (*equipment).into(),
            custom: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_muscles_known() {
        let all = builtin();
        let ids: HashSet<_> = all.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids.len(), all.len());
        assert!(all.iter().all(|e| MUSCLES.contains(&e.muscle.as_str())));
    }
}
