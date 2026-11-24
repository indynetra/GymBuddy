// Packages the Rust library (built by ../build-android.sh into src/main/jniLibs) as an APK.
plugins {
    id("com.android.application")
}

android {
    namespace = "dev.gymbuddy"
    compileSdk {
        version = release(37)
    }
    ndkVersion = "30.0.16248370"

    defaultConfig {
        applicationId = "dev.gymbuddy"
        minSdk = 26
        targetSdk = 36
        // CI passes these from the git tag; local builds use the defaults.
        versionCode = (findProperty("versionCode") as String?)?.toInt() ?: 1
        versionName = (findProperty("versionName") as String?) ?: "0.1.0"
    }

    // Release key comes from environment variables (GitHub secrets in CI).
    // Without them the release APK is signed with the debug key, which is fine for testing.
    val keystore = System.getenv("GYMBUDDY_KEYSTORE")
    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("GYMBUDDY_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("GYMBUDDY_KEY_ALIAS")
                keyPassword = System.getenv("GYMBUDDY_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName(if (keystore != null) "release" else "debug")
        }
    }

    packaging {
        jniLibs {
            useLegacyPackaging = false
        }
    }
}
