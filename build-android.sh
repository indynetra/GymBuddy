#!/usr/bin/env bash
# Builds the Android APK: Rust library via cargo-ndk, then packaged by Gradle.
# Usage: ./build-android.sh [--install]
set -euo pipefail
cd "$(dirname "$0")"

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
NDK_VERSION="30.0.16248370"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/$NDK_VERSION}"
# Slint's Android backend compiles a small Java helper and needs the platform jar.
export ANDROID_JAR="${ANDROID_JAR:-$(ls -d "$ANDROID_HOME"/platforms/android-*/android.jar | sort -V | tail -1)}"
export JAVA_HOME="${JAVA_HOME:-$(/usr/libexec/java_home 2>/dev/null || echo /opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home)}"

cargo ndk -t arm64-v8a -P 26 -o android/app/src/main/jniLibs build --release -p gymbuddy --lib

[ -f android/local.properties ] || echo "sdk.dir=$ANDROID_HOME" > android/local.properties
(cd android && ./gradlew assembleRelease --console=plain -q)

APK=android/app/build/outputs/apk/release/app-release.apk
echo "APK: $APK"
if [ "${1:-}" = "--install" ]; then
    adb install -r "$APK"
    adb shell am start -n dev.gymbuddy/android.app.NativeActivity
fi
