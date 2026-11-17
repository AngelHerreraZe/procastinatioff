// SECURITY: Verifier: has CAMERA and RECORD_AUDIO and does NOT have INTERNET (Android denies it the network).
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// Model hashes: a single source, `models.lock` ("<sha256>  <file>  <url>").
val modelHashes = file("models.lock").readLines()
    .map { it.substringBefore('#').trim() }
    .filter { it.isNotEmpty() }
    .joinToString(";") { line -> line.split(Regex("\\s+")).let { "${it[1]}=${it[0]}" } }

android {
    namespace = "dev.procastinatioff.verifier"
    // Reproducible builds (plan 09 · B.1): the dependency metadata block Google adds is not reproducible.
    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
    }
    compileSdk = 35
    ndkVersion = "27.2.12479018"

    defaultConfig {
        applicationId = "dev.procastinatioff.verifier"
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
        buildConfigField("String", "MODEL_HASHES", "\"$modelHashes\"")
    }
    // Per-app language picker (Android 13+), like the app
    androidResources { generateLocaleConfig = true }
    sourceSets["main"].res.srcDir("../branding/res") // launcher icon, shared with the app
    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        compose = true
        buildConfig = true
    }
    androidResources { noCompress += listOf("task", "tflite") } // models are read without decompressing
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.camerax.camera2)
    implementation(libs.camerax.lifecycle)
    implementation(libs.camerax.view)
    // pose, hands, face, objects, classifier. It declares the AutoValue *processor* (compile-time
    // only, with javapoet) as a runtime dependency: R8 fails on it and it does not belong in the APK.
    implementation(libs.mediapipe.tasks.vision) { exclude(group = "com.google.auto.value", module = "auto-value") }
    implementation(libs.mlkit.text.recognition)   // OCR with a bundled model (no download)
    implementation("${libs.jna.get()}@aar")       // procastinatioff-verify bindings
}

val buildVerifyRust by tasks.registering(Exec::class) {
    workingDir = rootProject.projectDir.parentFile
    commandLine(
        "cargo", "ndk", "-t", "arm64-v8a", "-t", "armeabi-v7a", "-t", "x86_64",
        "-o", "android/verifier/src/main/jniLibs", "build", "-p", "procastinatioff-verify", "--release",
    )
}
tasks.named("preBuild") { dependsOn(buildVerifyRust) }
