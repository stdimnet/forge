/**
 * A small Android app for browser mode, opened with `?project=android`: a Gradle (Kotlin
 * DSL, version catalog) build with the `app` module, its manifest and Kotlin sources, and a
 * Groovy release script. Modules, manifests, adb output, Android App command lines,
 * Logcat parsing and filtering and Groovy scripts are computed by the Rust crates
 * (`javaforge-android`, `javaforge-lang-groovy`) through WebAssembly (androidMock.ts,
 * jvmExtrasMock.ts); the device output is recorded.
 */
import type { ProjectInfo } from "@/ipc/project";

export const androidDemoName = "notes-app";

/** True when the page asks for the Android demo project. */
export const isAndroidDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "android";

export const androidDemoInfo: ProjectInfo = {
  languages: ["kotlin", "groovy", "java"],
  frameworks: [{ id: "android", name: "Android", version: "8.7.3", language: "java" }],
  packs: ["kotlinforge", "javaforge"],
  tools: ["git", "gradle", "android"],
  primary: { id: "android", name: "Android", version: "8.7.3", language: "java" },
};

export const androidDemoStartFile = "app/src/main/java/com/acme/notes/NoteRepository.kt";

export const androidDemoFiles: Record<string, string> = {
  "settings.gradle.kts": `pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "notes-app"
include(":app")
`,
  "build.gradle.kts": `plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.android) apply false
}
`,
  "gradle/libs.versions.toml": `[versions]
agp = "8.7.3"
kotlin = "2.1.0"
coreKtx = "1.15.0"
appcompat = "1.7.0"
junit = "4.13.2"

[libraries]
androidx-core-ktx = { group = "androidx.core", name = "core-ktx", version.ref = "coreKtx" }
androidx-appcompat = { group = "androidx.appcompat", name = "appcompat", version.ref = "appcompat" }
junit = { group = "junit", name = "junit", version.ref = "junit" }

[plugins]
android-application = { id = "com.android.application", version.ref = "agp" }
kotlin-android = { id = "org.jetbrains.kotlin.android", version.ref = "kotlin" }
`,
  "gradle.properties": `org.gradle.jvmargs=-Xmx2048m -Dfile.encoding=UTF-8
android.useAndroidX=true
kotlin.code.style=official
`,
  "gradlew": `#!/bin/sh
# Gradle start up script (wrapper)
exec java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain "$@"
`,
  "gradle/wrapper/gradle-wrapper.properties": `distributionUrl=https\\://services.gradle.org/distributions/gradle-8.11.1-bin.zip
`,
  ".gitignore": `*.iml
.gradle
/local.properties
/.idea
.DS_Store
/build
/captures
.cxx
`,
  "app/build.gradle.kts": `plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "com.acme.notes"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.acme.notes"
        minSdk = 26
        targetSdk = 35
        versionCode = 14
        versionName = "1.4.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.appcompat)
    testImplementation(libs.junit)
}
`,
  "app/proguard-rules.pro": `# Keep the note model for JSON.
-keep class com.acme.notes.Note { *; }
`,
  "app/src/main/AndroidManifest.xml": `<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:tools="http://schemas.android.com/tools">

    <uses-permission android:name="android.permission.INTERNET" />

    <application
        android:allowBackup="true"
        android:icon="@mipmap/ic_launcher"
        android:label="@string/app_name"
        android:theme="@style/Theme.Notes"
        tools:targetApi="35">
        <!-- <activity android:name=".OldActivity"> commented out -->
        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:theme="@style/Theme.Notes">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />

                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
        <activity android:name=".settings.SettingsActivity" android:exported="false" />
        <activity android:name="ShareActivity" android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.SEND" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:mimeType="text/plain" />
            </intent-filter>
        </activity>
    </application>

</manifest>
`,
  "app/src/main/res/values/strings.xml": `<resources>
    <string name="app_name">Notes</string>
    <string name="save">Save</string>
</resources>
`,
  "app/src/main/java/com/acme/notes/MainActivity.kt": `package com.acme.notes

import android.os.Bundle
import android.util.Log
import androidx.appcompat.app.AppCompatActivity

class MainActivity : AppCompatActivity() {
    private val repository = NoteRepository()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        Log.i("Notes", "Notes \${BuildConfig.VERSION_NAME} starting")
        repository.load()
    }

    fun onSaveClicked(title: String) {
        // The title comes straight from the text field.
        val note = Note(title = title, body = "")
        runOnUiThread {
            repository.save(note)
        }
    }
}
`,
  "app/src/main/java/com/acme/notes/NoteRepository.kt": `package com.acme.notes

import android.util.Log

data class Note(val title: String, val body: String)

class NoteRepository {
    private val notes = mutableListOf<Note>()

    fun load() {
        Log.d(TAG, "Loading \${notes.size} notes")
    }

    fun save(note: Note) {
        Log.d(TAG, "saving note \\"\${note.title}\\"")
        // A note needs a title before it can be stored.
        check(note.title.isNotBlank()) { "Note title must not be empty" }
        notes += note
    }

    private companion object {
        const val TAG = "NoteRepository"
    }
}
`,
  "app/src/test/java/com/acme/notes/NoteRepositoryTest.kt": `package com.acme.notes

import org.junit.Assert.assertThrows
import org.junit.Test

class NoteRepositoryTest {
    @Test
    fun rejectsEmptyTitles() {
        assertThrows(IllegalStateException::class.java) {
            NoteRepository().save(Note(title = "", body = "x"))
        }
    }
}
`,
  "scripts/release.groovy": `#!/usr/bin/env groovy
// Bumps the version name in app/build.gradle and prints the release notes.
import groovy.transform.Field

@Field String buildFile = 'app/build.gradle'

def version = args ? args[0] : '1.4.0'
def gradle = new File(buildFile)
gradle.text = gradle.text.replaceAll(/versionName "[^"]+"/, "versionName \\"$version\\"")

def notes(String v) {
    "Release $v: offline sync, faster search"
}

println "Bumped \${buildFile} to $version"
println notes(version)
`,
};

/** `adb devices -l` of the demo machine: an emulator, a phone and an unauthorized device. */
export const adbDevicesOutput = `List of devices attached
emulator-5554          device product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64 device:emu64xa transport_id:1
R58N12ABCDE            device usb:1-1 product:dm3qxxx model:SM_S918B device:dm3q transport_id:3
0a1b2c3d               unauthorized usb:1-2 transport_id:4

`;

/** `emulator -list-avds`. */
export const avdsOutput = `Pixel_8_API_35\nPixel_Tablet_API_34\nWear_OS_Small_Round_API_34\n`;

/** `adb shell ps -A -o PID,NAME` of the emulator. */
export const psOutput = `  PID NAME
    1 init
    2 kthreadd
  612 zygote64
 1432 system_server
 2210 com.android.systemui
12034 com.acme.notes
12101 com.acme.notes:sync
`;

/** The emulator's log (`adb logcat -v threadtime`): the app starts, syncs and crashes. */
export const logcatOutput = `--------- beginning of main
10-27 10:14:03.512  1432  1459 I ActivityManager: Start proc 12034:com.acme.notes/u0a190 for top-activity {com.acme.notes/com.acme.notes.MainActivity}
10-27 10:14:03.640 12034 12034 I Notes   : Notes 1.4.0 starting
10-27 10:14:03.702 12034 12051 D NoteRepository: Loading 42 notes
10-27 10:14:03.731  2210  2210 W Choreographer: Skipped 31 frames!  The application may be doing too much work on its main thread.
10-27 10:14:04.118 12034 12060 D OkHttp  : --> GET https://api.acme.com/notes
10-27 10:14:09.120 12034 12060 W OkHttp  : <-- HTTP FAILED: java.net.SocketTimeoutException: timeout
10-27 10:14:09.402  1432  1500 I ActivityManager: Start proc 13001:com.acme.notes:sync/u0a190 for service {com.acme.notes/com.acme.notes.sync.SyncService}
10-27 10:14:09.511 13001 13001 I SyncService: Sync started
--------- beginning of crash
10-27 10:14:10.004 12034 12034 E AndroidRuntime: FATAL EXCEPTION: main
10-27 10:14:10.004 12034 12034 E AndroidRuntime: Process: com.acme.notes, PID: 12034
10-27 10:14:10.004 12034 12034 E AndroidRuntime: java.lang.IllegalStateException: Note title must not be empty
10-27 10:14:10.004 12034 12034 E AndroidRuntime: 	at com.acme.notes.NoteRepository.save(NoteRepository.kt:17)
10-27 10:14:10.004 12034 12034 E AndroidRuntime: 	at com.acme.notes.MainActivity.onSaveClicked$lambda$2(MainActivity.kt:21)
10-27 10:14:10.004 12034 12034 E AndroidRuntime: 	at android.app.Activity.performCreate(Activity.java:8960)
10-27 10:14:10.004 12034 12034 E AndroidRuntime: 	... 12 more
10-27 10:14:10.010 12034 12051 D NoteRepository: saving note "Groceries" failed
`;
