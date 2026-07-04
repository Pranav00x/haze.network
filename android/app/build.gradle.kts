plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.haze.wallet"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.haze.wallet"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
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
        compose = true
    }
    composeOptions {
        kotlinCompilerExtensionVersion = "1.5.14"
    }

    packaging {
        // Only one architecture's copy of these should ever ship in an APK;
        // avoids "duplicate file" packaging failures across the native libs
        // pulled in transitively by JNA plus our own jniLibs.
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.4")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.4")
    implementation("androidx.activity:activity-compose:1.9.1")
    implementation(platform("androidx.compose:compose-bom:2024.06.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.navigation:navigation-compose:2.7.7")

    // UniFFI-generated bindings call into the native library through JNA.
    implementation("net.java.dev.jna:jna:5.14.0@aar")

    // HTTP client for the node's JSON API (mirrors the web wallet's fetch()).
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("org.json:json:20240303")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")

    // Encrypted local storage for the serialized keystore/wallet-store bytes
    // (Android Keystore-backed) - mirrors the web wallet's password-derived
    // AES-GCM encryption of the same bytes before persisting.
    implementation("androidx.security:security-crypto:1.1.0-alpha06")
}
