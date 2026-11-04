//! Update checks and signed updates (`tauri-plugin-updater`).
//!
//! The decisions (once a day, skipped and postponed versions, channel, version compare,
//! simulated updates) live in `forge-update`; this module reads the settings and the build's
//! public key, asks `forge-update`, and downloads, verifies and installs with the plugin.
//!
//! - The updater public key is compiled in from `FORGE_UPDATER_PUBKEY`. Builds without it
//!   never check ("Updates are not configured in this build").
//! - Debug builds never check. `FORGE_SIMULATE_UPDATE=0.2.0` shows a simulated offer with a
//!   fake download in any build; nothing is installed.
//! - Progress of a download goes out as `updates://download` (to every window).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use forge_core::{ForgeError, Result};
use forge_update::{
    simulate, CheckOutcome, CheckPlan, CheckRequest, PlanInput, SkipReason, SpeedMeter,
    UpdateOffer, UpdateState,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

use crate::startup::{self, SPLASH_LABEL};
use crate::state::AppState;

/// The updater's public key (minisign), set when the release is built.
const PUBKEY: Option<&str> = option_env!("FORGE_UPDATER_PUBKEY");
/// Download progress: [`DownloadProgress`].
pub const DOWNLOAD_EVENT: &str = "updates://download";
/// An update the splash window should offer: [`UpdateOffer`].
pub const OFFER_EVENT: &str = "splash://update";
/// Version to pretend is available (development).
pub const SIMULATE_ENV: &str = "FORGE_SIMULATE_UPDATE";

pub const KEY_CHECK_ON_STARTUP: &str = "updates.checkOnStartup";
pub const KEY_CHANNEL: &str = "updates.channel";
pub const KEY_ENDPOINT: &str = "updates.endpoint";

/// The configured public key, if any.
pub fn pubkey() -> Option<&'static str> {
    PUBKEY.map(str::trim).filter(|k| !k.is_empty())
}

#[derive(Default)]
pub struct UpdatesState {
    state: Mutex<Option<UpdateState>>,
    /// The offer the user has not answered yet.
    pending: Mutex<Option<UpdateOffer>>,
    downloading: AtomicBool,
    /// The startup thread waits here while the splash offers an update.
    waiter: Mutex<Option<mpsc::Sender<()>>>,
}

impl UpdatesState {
    fn with_state<T>(&self, f: impl FnOnce(&mut UpdateState) -> T) -> T {
        let mut guard = self.state.lock().expect("update state lock poisoned");
        let state = guard.get_or_insert_with(|| {
            state_path()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|text| UpdateState::from_json(&text))
                .unwrap_or_default()
        });
        f(state)
    }

    /// Change the state and save it.
    fn update_state(&self, f: impl FnOnce(&mut UpdateState)) {
        let json = self.with_state(|s| {
            f(s);
            s.to_json()
        });
        let Some(path) = state_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&path, json) {
            tracing::warn!("cannot save the update state: {e}");
        }
    }

    /// Let the startup thread continue (no-op outside startup).
    fn release_waiter(&self) {
        if let Some(tx) = self.waiter.lock().expect("waiter lock poisoned").take() {
            let _ = tx.send(());
        }
    }
}

fn state_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("forge").join("updates.json"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

/// How this build handles updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateMode {
    /// Checks the feed and installs signed updates.
    Enabled,
    /// `FORGE_SIMULATE_UPDATE` is set.
    Simulated,
    /// Debug build: no checks.
    DevBuild,
    /// No public key in this build.
    NotConfigured,
}

fn simulate_version() -> Option<String> {
    std::env::var(SIMULATE_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
}

fn mode() -> UpdateMode {
    if simulate_version().is_some() {
        UpdateMode::Simulated
    } else if cfg!(debug_assertions) {
        UpdateMode::DevBuild
    } else if pubkey().is_none() {
        UpdateMode::NotConfigured
    } else {
        UpdateMode::Enabled
    }
}

/// Feed keys of this installation, most specific first (`linux-x86_64-appimage`,
/// `linux-x86_64`).
fn targets() -> Vec<String> {
    let Some(base) = tauri_plugin_updater::target() else {
        return Vec::new();
    };
    use tauri::utils::{config::BundleType, platform::bundle_type};
    let installer = match bundle_type() {
        Some(BundleType::AppImage) => Some("appimage"),
        Some(BundleType::Deb) => Some("deb"),
        Some(BundleType::Rpm) => Some("rpm"),
        Some(BundleType::Msi) => Some("msi"),
        Some(BundleType::Nsis) => Some("nsis"),
        Some(BundleType::App) => Some("app"),
        _ => None,
    };
    let mut out = Vec::new();
    if let Some(i) = installer {
        out.push(format!("{base}-{i}"));
    }
    out.push(base);
    out
}

/// The primary feed of the selected channel (shown on the Updates settings page).
fn endpoint(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let settings = state.read_settings();
    forge_update::Channel::from_setting(settings.get_str(KEY_CHANNEL))
        .endpoint(settings.get_str(KEY_ENDPOINT))
}

/// Every feed the selected channel reads (the beta channel also reads the stable feed).
fn endpoints(app: &AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let settings = state.read_settings();
    forge_update::Channel::from_setting(settings.get_str(KEY_CHANNEL))
        .endpoints(settings.get_str(KEY_ENDPOINT))
}

/// The result of a check, for the splash and the Check for Updates dialog.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum CheckResult {
    Available {
        offer: UpdateOffer,
    },
    UpToDate {
        version: String,
    },
    /// No check ran (development build, not configured, turned off, checked today).
    #[serde(rename_all = "camelCase")]
    NotChecked {
        reason: SkipReason,
        message: String,
    },
    Error {
        message: String,
    },
}

fn skip_message(reason: &SkipReason) -> String {
    match reason {
        SkipReason::DevBuild => {
            "Development builds do not check for updates. Set FORGE_SIMULATE_UPDATE to try the update flow.".into()
        }
        SkipReason::NotConfigured => "Updates are not configured in this build.".into(),
        SkipReason::Disabled => "Checking for updates on startup is turned off.".into(),
        SkipReason::CheckedRecently { .. } => "Checked for updates less than a day ago.".into(),
    }
}

async fn run_check(app: &AppHandle, manual: bool) -> CheckResult {
    let updates = app.state::<UpdatesState>();
    let current = current_version(app);
    let check_on_startup = app
        .state::<AppState>()
        .read_settings()
        .get_bool(KEY_CHECK_ON_STARTUP)
        .unwrap_or(true);
    let input = PlanInput {
        manual,
        debug_build: cfg!(debug_assertions),
        simulate: simulate_version(),
        pubkey_configured: pubkey().is_some(),
        check_on_startup,
        now: now(),
    };
    let plan = updates.with_state(|s| forge_update::plan(&input, s));
    let result = match plan {
        CheckPlan::Simulate(version) => match simulate::offer(&version, &current) {
            Ok(offer) => CheckResult::Available { offer },
            Err(e) => CheckResult::Error {
                message: format!("{SIMULATE_ENV}: {e}"),
            },
        },
        CheckPlan::Skip(reason) => {
            if !manual {
                tracing::info!("update check skipped: {}", skip_message(&reason));
            }
            let message = skip_message(&reason);
            CheckResult::NotChecked { reason, message }
        }
        CheckPlan::Check => {
            let endpoints = endpoints(app);
            let channel = forge_update::Channel::from_setting(
                app.state::<AppState>().read_settings().get_str(KEY_CHANNEL),
            );
            let Ok(version) = forge_update::parse_version(&current) else {
                return CheckResult::Error {
                    message: format!("invalid app version {current}"),
                };
            };
            let targets = targets();
            let state = updates.with_state(|s| s.clone());
            let req = CheckRequest {
                endpoints: &endpoints,
                current: &version,
                channel,
                state: &state,
                manual,
                now: now(),
                targets: &targets,
            };
            let client = forge_update::client(&current);
            match forge_update::check(&client, &req).await {
                Ok(outcome) => {
                    updates.update_state(|s| s.record_check(now()));
                    match outcome {
                        CheckOutcome::Available { offer } => CheckResult::Available { offer },
                        CheckOutcome::UpToDate | CheckOutcome::NotOffered { .. } => {
                            CheckResult::UpToDate { version: current }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("update check failed ({}): {e}", endpoints.join(", "));
                    CheckResult::Error {
                        message: e.short_message(),
                    }
                }
            }
        }
    };
    if let CheckResult::Available { offer } = &result {
        *updates.pending.lock().expect("pending lock poisoned") = Some(offer.clone());
    }
    result
}

/// The update step of the splash: check (at most once a day) and return an offer.
pub fn startup_check(app: &AppHandle) -> Option<UpdateOffer> {
    let detail = match mode() {
        UpdateMode::Simulated => "simulated".to_string(),
        UpdateMode::DevBuild => SkipReason::DevBuild.detail().to_string(),
        UpdateMode::NotConfigured => {
            tracing::info!(
                "update checks are disabled: this build has no updater public key (FORGE_UPDATER_PUBKEY)"
            );
            SkipReason::NotConfigured.detail().to_string()
        }
        UpdateMode::Enabled => url_host(&endpoint(app)),
    };
    startup::progress(app, "Checking for updates", detail, 78);
    match tauri::async_runtime::block_on(run_check(app, false)) {
        CheckResult::Available { offer } => Some(offer),
        CheckResult::UpToDate { .. } => {
            startup::progress(app, "Up to date", "", 86);
            None
        }
        CheckResult::NotChecked { reason, .. } => {
            startup::progress(app, "Checking for updates", reason.detail(), 86);
            None
        }
        CheckResult::Error { message } => {
            startup::progress(app, "Update check failed", message, 86);
            std::thread::sleep(Duration::from_millis(1200));
            None
        }
    }
}

fn url_host(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
        .to_string()
}

/// Show `offer` on the splash and wait until the user answered (or a download failed).
pub fn offer_on_splash(app: &AppHandle, offer: UpdateOffer) {
    let updates = app.state::<UpdatesState>();
    let (tx, rx) = mpsc::channel();
    *updates.waiter.lock().expect("waiter lock poisoned") = Some(tx);
    *updates.pending.lock().expect("pending lock poisoned") = Some(offer.clone());
    startup::progress(app, "Update available", offer.version.clone(), 88);
    let _ = app.emit_to(SPLASH_LABEL, OFFER_EVENT, offer);
    // The splash cannot answer once it is gone; never block startup for longer than this.
    if rx.recv_timeout(Duration::from_secs(30 * 60)).is_err() {
        tracing::warn!("no answer to the update offer; continuing");
    }
}

/// The offer the splash still has to answer.
pub fn waiting_offer(app: &AppHandle) -> Option<UpdateOffer> {
    let updates = app.state::<UpdatesState>();
    if updates
        .waiter
        .lock()
        .expect("waiter lock poisoned")
        .is_none()
    {
        return None;
    }
    let offer = updates
        .pending
        .lock()
        .expect("pending lock poisoned")
        .clone();
    offer
}

/// Progress of a download, sent as `updates://download`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub version: String,
    pub phase: DownloadPhase,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub bytes_per_second: u64,
    /// Error text (`failed`) or what happened (`simulated`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadPhase {
    Downloading,
    /// Checking the signature against the build's public key.
    Verifying,
    /// The signature is valid.
    Verified,
    Installing,
    Restarting,
    /// A simulated update finished; nothing was installed.
    Simulated,
    Failed,
}

struct Reporter<'a> {
    app: &'a AppHandle,
    version: String,
    total: Option<u64>,
    downloaded: u64,
    meter: SpeedMeter,
    started: Instant,
    last_emit: Option<Instant>,
    speed: u64,
}

impl<'a> Reporter<'a> {
    fn new(app: &'a AppHandle, version: &str, total: Option<u64>) -> Self {
        Self {
            app,
            version: version.to_string(),
            total,
            downloaded: 0,
            meter: SpeedMeter::new(),
            started: Instant::now(),
            last_emit: None,
            speed: 0,
        }
    }

    fn chunk(&mut self, bytes: u64, total: Option<u64>) {
        self.downloaded += bytes;
        if total.is_some() {
            self.total = total;
        }
        let at = self.started.elapsed().as_millis() as u64;
        self.speed = self.meter.record(at, self.downloaded);
        // At most ~10 events per second.
        let due = match self.last_emit {
            Some(t) => t.elapsed() >= Duration::from_millis(100),
            None => true,
        };
        if due {
            self.last_emit = Some(Instant::now());
            self.emit(DownloadPhase::Downloading, None);
        }
    }

    fn emit(&self, phase: DownloadPhase, message: Option<String>) {
        let p = DownloadProgress {
            version: self.version.clone(),
            phase,
            downloaded: self.downloaded,
            total: self.total,
            bytes_per_second: self.speed,
            message,
        };
        if let Err(e) = self.app.emit(DOWNLOAD_EVENT, p) {
            tracing::warn!("cannot emit {DOWNLOAD_EVENT}: {e}");
        }
    }
}

/// Short text for a failed download or install.
fn download_error(e: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    match e {
        E::Minisign(_)
        | E::Base64(_)
        | E::SignatureUtf8(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion => {
            "The signature of the download is not valid. Nothing was installed.".into()
        }
        E::Reqwest(_) | E::Network(_) => "The download failed. Are you offline?".into(),
        E::ReleaseNotFound => "The update is no longer available.".into(),
        other => format!("The update could not be installed: {other}"),
    }
}

async fn download_real(app: &AppHandle, offer: &UpdateOffer) -> std::result::Result<(), String> {
    let url = offer
        .feed
        .clone()
        .unwrap_or_else(|| endpoint(app))
        .parse()
        .map_err(|e| format!("invalid update endpoint: {e}"))?;
    let wanted = offer.version.clone();
    let mut builder = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(|e| download_error(&e))?
        .version_comparator(move |_, release| release.version.to_string() == wanted);
    if let Some(key) = pubkey() {
        builder = builder.pubkey(key);
    }
    let updater = builder.build().map_err(|e| download_error(&e))?;
    let update = updater
        .check()
        .await
        .map_err(|e| download_error(&e))?
        .ok_or_else(|| "The update is no longer available.".to_string())?;
    let mut reporter = Reporter::new(app, &offer.version, offer.size);
    let reporter_ref = Mutex::new(&mut reporter);
    let bytes = update
        .download(
            |chunk, total| {
                reporter_ref
                    .lock()
                    .expect("reporter lock poisoned")
                    .chunk(chunk as u64, total)
            },
            || {
                reporter_ref
                    .lock()
                    .expect("reporter lock poisoned")
                    .emit(DownloadPhase::Verifying, None)
            },
        )
        .await
        .map_err(|e| download_error(&e))?;
    reporter.emit(DownloadPhase::Verified, None);
    reporter.emit(DownloadPhase::Installing, None);
    update.install(bytes).map_err(|e| download_error(&e))?;
    reporter.emit(DownloadPhase::Restarting, None);
    tokio::time::sleep(Duration::from_millis(600)).await;
    app.restart();
}

async fn download_simulated(app: &AppHandle, offer: &UpdateOffer) {
    let plan = simulate::SimulatedDownload::new(offer.size.unwrap_or(simulate::SIMULATED_SIZE));
    let mut reporter = Reporter::new(app, &offer.version, Some(plan.total()));
    for chunk in plan {
        tokio::time::sleep(Duration::from_millis(simulate::CHUNK_INTERVAL_MS)).await;
        reporter.last_emit = None;
        reporter.chunk(chunk, None);
    }
    reporter.emit(DownloadPhase::Verifying, None);
    tokio::time::sleep(Duration::from_millis(700)).await;
    reporter.emit(DownloadPhase::Verified, None);
    tokio::time::sleep(Duration::from_millis(800)).await;
    reporter.emit(
        DownloadPhase::Simulated,
        Some("Simulated update: nothing was installed.".into()),
    );
}

/// Run the download of the pending offer, then restart (or report the failure).
async fn download(app: AppHandle, offer: UpdateOffer) {
    let updates = app.state::<UpdatesState>();
    if offer.simulated {
        download_simulated(&app, &offer).await;
        tokio::time::sleep(Duration::from_millis(1500)).await;
    } else if let Err(message) = download_real(&app, &offer).await {
        tracing::warn!("update {} failed: {message}", offer.version);
        Reporter::new(&app, &offer.version, offer.size).emit(DownloadPhase::Failed, Some(message));
        // Give the user a moment to read the message before the workbench opens.
        tokio::time::sleep(Duration::from_millis(2500)).await;
    }
    updates.downloading.store(false, Ordering::SeqCst);
    updates.release_waiter();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateAction {
    /// Download, verify, install and restart.
    Download,
    /// Ask again tomorrow.
    Later,
    /// Never offer this version automatically.
    Skip,
}

/// The user's answer to the pending update offer (splash or Check for Updates dialog).
#[tauri::command]
pub async fn updates_respond(
    action: UpdateAction,
    app: AppHandle,
    updates: State<'_, UpdatesState>,
) -> Result<()> {
    let offer = updates
        .pending
        .lock()
        .expect("pending lock poisoned")
        .clone()
        .ok_or_else(|| err("no update is pending"))?;
    match action {
        UpdateAction::Later => {
            if !offer.simulated {
                updates.update_state(|s| s.postpone(&offer.version, now()));
            }
            startup::progress(&app, "Update postponed", "asks again tomorrow", 90);
        }
        UpdateAction::Skip => {
            if !offer.simulated {
                updates.update_state(|s| s.skip(&offer.version));
            }
            let detail = "no reminders for this version";
            startup::progress(&app, &format!("Skipped {}", offer.version), detail, 90);
        }
        UpdateAction::Download => {
            if updates.downloading.swap(true, Ordering::SeqCst) {
                return Err(err("the update is already downloading"));
            }
            tauri::async_runtime::spawn(download(app.clone(), offer));
            return Ok(());
        }
    }
    *updates.pending.lock().expect("pending lock poisoned") = None;
    updates.release_waiter();
    Ok(())
}

/// Help | Check for Updates…: check now (ignores the daily limit and skipped versions).
#[tauri::command]
pub async fn updates_check(app: AppHandle) -> CheckResult {
    run_check(&app, true).await
}

/// What the Updates settings page shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesStatus {
    pub mode: UpdateMode,
    pub current_version: String,
    pub endpoint: String,
    /// Unix seconds.
    pub last_check: Option<u64>,
    pub skipped_version: Option<String>,
}

#[tauri::command]
pub fn updates_status(app: AppHandle, updates: State<'_, UpdatesState>) -> UpdatesStatus {
    let (last_check, skipped_version) =
        updates.with_state(|s| (s.last_check, s.skipped_version.clone()));
    UpdatesStatus {
        mode: mode(),
        current_version: current_version(&app),
        endpoint: endpoint(&app),
        last_check,
        skipped_version,
    }
}

/// Offer the skipped version again.
#[tauri::command]
pub fn updates_clear_skipped(updates: State<'_, UpdatesState>) {
    updates.update_state(|s| s.clear_skipped());
}
