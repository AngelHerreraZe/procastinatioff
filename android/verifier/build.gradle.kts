// 🔒 Verificador: tiene CAMERA y RECORD_AUDIO y NO tiene INTERNET (Android le niega la red).
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// Hashes de los modelos: una sola fuente, `models.lock` ("<sha256>  <archivo>  <url>").
val modelHashes = file("models.lock").readLines()
    .map { it.substringBefore('#').trim() }
    .filter { it.isNotEmpty() }
    .joinToString(";") { line -> line.split(Regex("\\s+")).let { "${it[1]}=${it[0]}" } }

android {
    namespace = "dev.procastinacioff.verifier"
    compileSdk = 35
    ndkVersion = "27.2.12479018"

    defaultConfig {
        applicationId = "dev.procastinacioff.verifier"
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
        buildConfigField("String", "MODEL_HASHES", "\"$modelHashes\"")
    }
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
    kotlinOptions { jvmTarget = "17" }
    buildFeatures {
        compose = true
        buildConfig = true
    }
    androidResources { noCompress += listOf("task", "tflite") } // los modelos se leen sin descomprimir
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.camerax.camera2)
    implementation(libs.camerax.lifecycle)
    implementation(libs.camerax.view)
    implementation(libs.mediapipe.tasks.vision)   // pose, manos, rostro, objetos, clasificador
    implementation(libs.mlkit.text.recognition)   // OCR con modelo empaquetado (sin descarga)
    implementation("${libs.jna.get()}@aar")       // bindings de procastinacioff-verify
}

val buildVerifyRust by tasks.registering(Exec::class) {
    workingDir = rootProject.projectDir.parentFile
    commandLine(
        "cargo", "ndk", "-t", "arm64-v8a", "-t", "armeabi-v7a", "-t", "x86_64",
        "-o", "android/verifier/src/main/jniLibs", "build", "-p", "procastinacioff-verify", "--release",
    )
}
tasks.named("preBuild") { dependsOn(buildVerifyRust) }
