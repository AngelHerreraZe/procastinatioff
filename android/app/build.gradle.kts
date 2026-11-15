plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// FCM solo si existe google-services.json (proyecto Firebase con SOLO Cloud Messaging habilitado).
// Sin él la app funciona igual; la sincronización se hace cada 15 min y al abrir la app.
if (file("google-services.json").exists()) {
    apply(plugin = "com.google.gms.google-services")
}

android {
    namespace = "dev.procastinacioff.app"
    compileSdk = 35
    ndkVersion = "27.2.12479018"

    defaultConfig {
        applicationId = "dev.procastinacioff.app"
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures { compose = true }
    packaging { jniLibs { useLegacyPackaging = false } }
    testOptions { unitTests.isReturnDefaultValues = true }
    // Los bindings generados de UniFFI usan java.lang.ref.Cleaner con comprobación en tiempo de ejecución
    lint { lintConfig = file("lint.xml") }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    implementation(libs.navigation.compose)
    implementation(libs.work.runtime)
    implementation(libs.coroutines.android)
    implementation("${libs.jna.get()}@aar")          // requerido por los bindings de UniFFI
    implementation(libs.camerax.camera2)              // solo para escanear el QR de vinculación
    implementation(libs.camerax.lifecycle)
    implementation(libs.camerax.view)
    implementation(libs.zxing.core)                   // decodifica/genera QR en el dispositivo
    implementation(libs.biometric)                    // modo familiar: el tutor confirma con huella o PIN
    implementation(libs.fragment)
    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.messaging)           // SOLO mensajería; nada de Analytics
    testImplementation(libs.junit)
    testImplementation(libs.jna)                      // los tipos de UniFFI cargan JNA en la JVM de pruebas
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.espresso.core)
}

// Compila el núcleo Rust antes de cada build (necesita cargo-ndk y el NDK).
// En Windows sin Strawberry Perl, compilar con `-x buildRust` y jniLibs ya generados (CI: Linux).
val buildRust by tasks.registering(Exec::class) {
    workingDir = rootProject.projectDir.parentFile   // raíz del monorepo
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a", "-t", "armeabi-v7a", "-t", "x86_64",
        "-o", "android/app/src/main/jniLibs",
        "build", "-p", "procastinacioff-core", "--release",
    )
}
tasks.named("preBuild") { dependsOn(buildRust) }
