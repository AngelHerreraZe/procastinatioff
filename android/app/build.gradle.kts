plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "dev.procastinatioff.app"
    // Reproducible builds (plan 09 · B.1): the dependency metadata block Google adds is not reproducible.
    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
    }
    compileSdk = 36 // Health Connect 1.1 needs it; targetSdk stays at 35
    ndkVersion = "27.2.12479018"

    defaultConfig {
        applicationId = "dev.procastinatioff.app"
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
    buildFeatures { compose = true }
    // Per-app language picker (Android 13+): built from the values-<lang>/ folders. Source language: res/resources.properties
    androidResources {
        generateLocaleConfig = true
        noCompress += "apk" // the embedded verifier is already a zip
    }
    sourceSets["main"].res.srcDir("../branding/res") // launcher icon, shared with the verifier
    packaging { jniLibs { useLegacyPackaging = false } }
    testOptions { unitTests.isReturnDefaultValues = true }
    // The generated UniFFI bindings use java.lang.ref.Cleaner with a runtime check
    lint { lintConfig = file("lint.xml") }
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }

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
    implementation("${libs.jna.get()}@aar")          // required by the UniFFI bindings
    implementation(libs.camerax.camera2)              // only to scan the pairing QR
    implementation(libs.camerax.lifecycle)
    implementation(libs.camerax.view)
    implementation(libs.zxing.core)                   // decodes/generates QR codes on the device
    implementation(libs.biometric)                    // family mode: the guardian confirms with fingerprint or PIN
    implementation(libs.fragment)
    implementation(libs.health.connect)                // "Workout" goals: today's exercise minutes (read only)
    implementation(libs.mlkit.genai.prompt)            // goal check suggestions with the phone's own Gemini Nano (AICore)
    testImplementation(libs.junit)
    testImplementation(libs.jna)                      // the UniFFI types load JNA in the test JVM
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.espresso.core)
}

// SECURITY: The camera verifier stays a separate APK WITHOUT Internet, but it ships inside this one
// (assets/verifier.apk, same build type) and onboarding installs it first, before blocking starts (VerifierScreen, VerifierLauncher.kt).
// Release: CI embeds the unsigned verifier; the signing ceremony swaps in the signed one (RELEASING.md).
abstract class EmbedVerifier : DefaultTask() {
    @get:InputFiles abstract val apks: ConfigurableFileCollection
    @get:OutputDirectory abstract val out: DirectoryProperty

    @TaskAction fun copy() {
        apks.asFileTree.matching { include("*.apk") }.singleFile
            .copyTo(out.file("verifier.apk").get().asFile, overwrite = true)
    }
}
androidComponents.onVariants { variant ->
    val type = variant.buildType ?: return@onVariants
    val embed = tasks.register<EmbedVerifier>("embed${variant.name.replaceFirstChar(Char::uppercase)}Verifier") {
        apks.from(rootProject.layout.projectDirectory.dir("verifier/build/outputs/apk/$type"))
            .builtBy(":verifier:assemble${type.replaceFirstChar(Char::uppercase)}")
    }
    variant.sources.assets?.addGeneratedSourceDirectory(embed, EmbedVerifier::out)
}

// Builds the Rust core before every build (needs cargo-ndk and the NDK).
// On Windows without Strawberry Perl, build with `-x buildRust` and prebuilt jniLibs (CI: Linux).
val buildRust by tasks.registering(Exec::class) {
    workingDir = rootProject.projectDir.parentFile   // monorepo root
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a", "-t", "armeabi-v7a", "-t", "x86_64",
        "-o", "android/app/src/main/jniLibs",
        "build", "-p", "procastinatioff-core", "--release",
    )
}
tasks.named("preBuild") { dependsOn(buildRust) }
