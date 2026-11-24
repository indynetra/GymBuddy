# GymBuddy

An offline, private workout tracker written in Rust with a [Slint](https://slint.dev) UI.
One codebase runs on Android, macOS, Windows and Linux.

**Free for everyone, forever:** no ads, no account, no subscription, no tracking. Your data never leaves your device.

## Download

Get the latest APK from the [Releases](../../releases) page and open it on your phone
(allow "Install unknown apps" for your browser or file manager when asked).

## Features

- Live workout logging: weight × reps per set, tick to complete, "previous" column from your last session (tap to copy)
- Set types: tap the set number to cycle normal → warm-up (W) → drop set (D) → failure (F). Warm-ups are excluded from volume and records
- Rest timer starts when you tick a set (per-exercise rest time, ±15 s, skip)
- Routines: build from scratch or save any finished workout as a routine; start pre-fills weights from your last session
- History with per-workout detail; repeat a past workout
- Exercise library (55 built-in + custom exercises) with best set, estimated 1RM (Epley) and a progress chart
- Stats: weekly volume, week streak, sets per muscle (30 days), personal records on the finish screen
- kg / lb toggle, default rest time
- **Import from Hevy** (the CSV from Hevy's "Export workouts"). Duplicate workouts are skipped

Data is a single JSON file (`gymbuddy.json`), saved atomically. A corrupt file is moved aside, never overwritten.

## Layout

```
core/      gymbuddy-core: model, storage, stats, Hevy import (no UI, unit-tested)
app/       gymbuddy: Slint UI (ui/app.slint) + Rust glue (src/lib.rs); desktop main + android_main
android/   Gradle wrapper that packages the Rust library as an APK (NativeActivity, no Java code)
```

## Desktop

```bash
cargo run -p gymbuddy --release
```

Data lives in `~/.gymbuddy/` (override with `GYMBUDDY_DATA`). Import a Hevy export without the UI:

```bash
cargo run -p gymbuddy --release -- --import-hevy ~/Downloads/workouts.csv
```

## Android

Needs: Android SDK + NDK 30, `rustup target add aarch64-linux-android`, `cargo install cargo-ndk`, JDK 17+.

```bash
./build-android.sh --install     # builds, installs and launches on the connected device/emulator
```

The APK is `android/app/build/outputs/apk/release/app-release.apk` (signed with the debug key — use your own key before publishing).

To import Hevy data on the phone, copy the CSV to `Android/data/dev.gymbuddy/files/hevy.csv`
(e.g. `adb push workouts.csv /sdcard/Android/data/dev.gymbuddy/files/hevy.csv`) and tap **Stats → Import CSV**.

## Tests

```bash
cargo test
```

## Making a release

Releases are built by GitHub Actions (`.github/workflows/release.yml`) when you push a version tag:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

The APK must always be signed with the same key, or phones will refuse to install updates.
Create the key once, keep it (and its password) somewhere safe outside the repo, and add it as repository secrets:

```bash
keytool -genkeypair -v -keystore gymbuddy-release.jks -alias gymbuddy -keyalg RSA -keysize 4096 -validity 10000
base64 -i gymbuddy-release.jks | pbcopy   # paste as GYMBUDDY_KEYSTORE_BASE64
```

Secrets: `GYMBUDDY_KEYSTORE_BASE64`, `GYMBUDDY_KEYSTORE_PASSWORD`, `GYMBUDDY_KEY_ALIAS` (`gymbuddy`), `GYMBUDDY_KEY_PASSWORD`.
Without them the workflow still builds, but signs with a throwaway debug key.
To sign locally, set `GYMBUDDY_KEYSTORE` to the .jks path plus the same three password/alias variables before `./build-android.sh`.

## Contributing

Issues and pull requests are welcome. Run `cargo test` before sending a change.

## License

Copyright (C) 2026 GymBuddy contributors.

GymBuddy is free software: you can redistribute it and/or modify it under the terms of the
GNU General Public License as published by the Free Software Foundation, either version 3
of the License, or (at your option) any later version. See [LICENSE](LICENSE).

The UI is built with [Slint](https://slint.dev), used under its GPL-3.0 license.
GymBuddy is not affiliated with Hevy; the import reads the CSV that Hevy lets you export.
