import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.baselineprofile)
}

// A release keystore is optional and never committed: `keystore.properties`
// (storeFile, storePassword, keyAlias, keyPassword) beside this project's
// settings.gradle.kts. Without it, release builds are signed with the debug
// key so `assembleRelease` and `scripts/install_android.sh` still work.
val keystoreProperties = Properties().apply {
    val file = rootProject.file("keystore.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}

// The sync server new accounts go to. `-PprioritySyncServer=https://…` (or the
// same key in gradle.properties) builds an app that points somewhere else;
// Settings can still be told another server at run time.
val prioritySyncServer = (findProperty("prioritySyncServer") as String?)?.trim()?.takeIf { it.isNotEmpty() }
    ?: "https://api.takt.maybeitssoftware.co.uk"

// Accounts are Supabase Auth users (docs/sync.md). The publishable key is made
// to ship in apps. Each can be pointed elsewhere with a gradle property.
fun stringProperty(name: String): String? = (findProperty(name) as String?)?.trim()?.takeIf { it.isNotEmpty() }
val prioritySupabaseUrl = stringProperty("prioritySupabaseUrl") ?: "https://rsckzmldfpfjdrvulwke.supabase.co"
val prioritySupabaseKey = stringProperty("prioritySupabaseKey") ?: "sb_publishable_htC171zOquUGx7bBi8MrIQ_ZuIdqL5U"

// `local.properties` beside settings.gradle.kts is gitignored, so a machine's
// own values (the SDK path, a Google client id) can live there.
val localProperties = Properties().apply {
    val file = rootProject.file("local.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}

// The Google Cloud *Web* OAuth client id that Sign in with Google asks for an
// ID token for (Supabase checks the token's audience against it). Empty hides
// the Google button. The first of these that is set wins: the Gradle property
// (`-PpriorityGoogleWebClientId=…` or ~/.gradle/gradle.properties), the
// TAKT_GOOGLE_WEB_CLIENT_ID environment variable (CI), then
// `priorityGoogleWebClientId=…` in local.properties. docs/android-google-sign-in.md
// has the console steps that make one.
val priorityGoogleWebClientId = stringProperty("priorityGoogleWebClientId")
    ?: System.getenv("TAKT_GOOGLE_WEB_CLIENT_ID")?.trim()?.takeIf { it.isNotEmpty() }
    ?: localProperties.getProperty("priorityGoogleWebClientId")?.trim()?.takeIf { it.isNotEmpty() }
    ?: ""

// RevenueCat's public Google Play SDK key (`goog_…`), for Takt Sync's £1-a-year
// purchase. Empty: the purchase says it isn't available in this build. The
// Gradle property `taktRevenueCatKey`, then TAKT_REVENUECAT_ANDROID_KEY, then
// `taktRevenueCatKey=…` in local.properties.
val taktRevenueCatKey = stringProperty("taktRevenueCatKey")
    ?: System.getenv("TAKT_REVENUECAT_ANDROID_KEY")?.trim()?.takeIf { it.isNotEmpty() }
    ?: localProperties.getProperty("taktRevenueCatKey")?.trim()?.takeIf { it.isNotEmpty() }
    ?: ""

android {
    namespace = "uk.co.maybeitsadam.takt"
    compileSdk = 36
    defaultConfig {
        applicationId = "uk.co.maybeitssoftware.takt"
        minSdk = 29
        targetSdk = 36
        // scripts/build_play_bundle.sh passes the commit count, which only grows,
        // because Play refuses an upload whose versionCode it has seen before.
        versionCode = (findProperty("versionCode") as String?)?.toInt() ?: 1
        versionName = (findProperty("versionName") as String?) ?: "0.4.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        buildConfigField("String", "SYNC_SERVER", "\"$prioritySyncServer\"")
        buildConfigField("String", "SUPABASE_URL", "\"$prioritySupabaseUrl\"")
        buildConfigField("String", "SUPABASE_PUBLISHABLE_KEY", "\"$prioritySupabaseKey\"")
        buildConfigField("String", "GOOGLE_WEB_CLIENT_ID", "\"$priorityGoogleWebClientId\"")
        buildConfigField("String", "REVENUECAT_KEY", "\"$taktRevenueCatKey\"")
    }
    signingConfigs {
        if (keystoreProperties.isNotEmpty()) {
            create("release") {
                storeFile = rootProject.file(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("storePassword")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
            }
        }
    }
    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
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
    testOptions {
        unitTests.isReturnDefaultValues = true
    }
    packaging {
        resources.excludes += setOf("META-INF/LICENSE*", "META-INF/NOTICE*", "META-INF/*.kotlin_module")
        // JNA's desktop natives arrive with the test jar; the app needs only the
        // Android ones from the AAR.
        resources.excludes += setOf("com/sun/jna/**/*.dylib", "com/sun/jna/**/*.dll", "com/sun/jna/**/*.so")
    }
}

kotlin {
    jvmToolchain(17)
}

baselineProfile {
    // Generated by :baselineprofile on a connected device and committed under src/release/generated.
    automaticGenerationDuringBuild = false
    saveInSrc = true
    dexLayoutOptimization = true
}

composeCompiler {
    stabilityConfigurationFiles.add(project.layout.projectDirectory.file("compose-stability.conf"))
}

dependencies {
    implementation(project(":data"))
    implementation(project(":wearcontract"))
    implementation(libs.revenuecat.purchases)
    baselineProfile(project(":baselineprofile"))
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.process)
    implementation(libs.androidx.lifecycle.service)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.androidx.datastore.preferences)
    implementation(libs.androidx.work.runtime)
    implementation(libs.androidx.glance.appwidget)
    implementation(libs.androidx.glance.material3)
    implementation(libs.supabase.auth)
    implementation(libs.ktor.client.okhttp)
    implementation(libs.androidx.credentials)
    implementation(libs.androidx.credentials.play.services)
    implementation(libs.googleid)
    implementation(libs.play.services.auth)
    implementation(libs.play.services.wearable)
    implementation(libs.kotlinx.coroutines.play.services)
    implementation(libs.kotlinx.collections.immutable)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.androidx.profileinstaller)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons.core)
    implementation(libs.compose.material3.adaptive.navigation.suite)
    implementation(libs.compose.material3.window.size)
    implementation(libs.adaptive)
    implementation(libs.adaptive.layout)
    implementation(libs.adaptive.navigation)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.kotlinx.coroutines.android)
    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    // The availability and ranking rules call the Rust core through JNA; the
    // JVM tests need the desktop jar, since the AAR's natives are Android's.
    testImplementation(libs.jna)
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.kotlinx.coroutines.test)
}

kotlin {
    compilerOptions {
        optIn.addAll(
            "kotlinx.coroutines.ExperimentalCoroutinesApi",
            "kotlinx.coroutines.FlowPreview",
            "androidx.compose.material3.ExperimentalMaterial3Api",
            "androidx.compose.foundation.ExperimentalFoundationApi",
            "androidx.compose.foundation.layout.ExperimentalLayoutApi",
            "androidx.compose.material3.adaptive.ExperimentalMaterial3AdaptiveApi",
        )
    }
}

tasks.withType<Test>().configureEach {
    dependsOn(":data:buildRustCore", ":data:unpackSqliteHostNatives")
    // The desktop SQLite natives :data unpacks, for the tests that open a real workspace.
    systemProperty(
        "java.library.path",
        project(":data").layout.buildDirectory.dir("sqliteHostNatives").get().asFile.absolutePath,
    )
    // The host build of the core scripts/build_core_android.sh leaves in core/target.
    systemProperty("jna.library.path", rootProject.file("../../core/target/release").absolutePath)
}
