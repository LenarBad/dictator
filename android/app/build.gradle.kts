import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

val keystorePropertiesFile = rootProject.file("keystore.properties")
val keystoreProperties = Properties()
if (keystorePropertiesFile.exists()) {
    keystorePropertiesFile.inputStream().use { keystoreProperties.load(it) }
}

android {
    namespace = "io.lenar.dictator"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.lenar.dictator"
        minSdk = 29
        targetSdk = 35
        versionCode = 6
        versionName = "0.1.0"
    }

    signingConfigs {
        create("sideload") {
            if (keystorePropertiesFile.exists()) {
                storeFile = rootProject.file(keystoreProperties["storeFile"] as String)
                storePassword = keystoreProperties["storePassword"] as String
                keyAlias = keystoreProperties["keyAlias"] as String
                keyPassword = keystoreProperties["keyPassword"] as String
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (keystorePropertiesFile.exists()) {
                signingConfigs.getByName("sideload")
            } else {
                signingConfigs.getByName("debug")
            }
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        viewBinding = true
        aidl = true
    }

    packaging {
        jniLibs {
            useLegacyPackaging = false
        }
    }

    // Prefer uncompressed native libs in the APK (extractNativeLibs=false).
    // AGP sets this from packaging; do not put extractNativeLibs in the Manifest.

    androidResources {
        noCompress += listOf("onnx", "txt")
    }

    defaultConfig {
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.constraintlayout:constraintlayout:2.2.0")
    // Stay in :stt only — never import com.k2fsa.sherpa.onnx from the IME process.
    implementation("com.github.k2-fsa.sherpa-onnx:sherpa-onnx:v1.13.8")
    testImplementation("junit:junit:4.13.2")
}
