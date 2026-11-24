use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default()
}

fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("GYMBUDDY_DATA") {
        return dir.into();
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("GymBuddy");
    }
    home().join(".gymbuddy")
}

fn main() {
    let dir = data_dir();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [flag, path] = args.as_slice() {
        if flag == "--import-hevy" {
            match gymbuddy::import_hevy_file(&dir, path.as_ref()) {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            return;
        }
    }
    let import_dirs = vec![home().join("Downloads"), dir.clone()];
    if let Err(e) = gymbuddy::run(dir, import_dirs) {
        eprintln!("GymBuddy failed: {e}");
        std::process::exit(1);
    }
}
