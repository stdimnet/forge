//! JavaForge's Android basics, like Android Studio's. Pure Rust (no file system or
//! processes), so the desktop backend and the browser mode (WebAssembly) share it.
//!
//! - [`gradle`] recognizes Gradle Android modules (the Android Gradle Plugin, namespace,
//!   application id, SDK levels, the AGP version); [`manifest`] reads `AndroidManifest.xml`
//!   (package, activities, the launcher activity).
//! - [`sdk`] finds the Android SDK (setting, `ANDROID_HOME`, `ANDROID_SDK_ROOT`,
//!   `local.properties`, the default folder) and its tools; Forge never downloads it.
//! - [`adb`] reads `adb devices -l`, `emulator -list-avds` and `adb shell ps`.
//! - [`run`] builds the "Android App" run: assemble with Gradle, `adb install -r`, start the
//!   launcher activity.
//! - [`logcat`] parses `adb logcat -v threadtime`, tracks the packages of processes, links
//!   stack trace frames to project files and filters with Android Studio's query language.

pub mod adb;
pub mod gradle;
pub mod logcat;
pub mod manifest;
pub mod run;
pub mod sdk;

pub use adb::Device;
pub use logcat::{LogEntry, Logcat, LogcatView, Query};
