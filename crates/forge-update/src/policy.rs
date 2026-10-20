//! When to check for updates and which release is an update.

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{BETA_ENDPOINT, CHECK_INTERVAL_SECS, DEFAULT_ENDPOINT};

/// The `updates.channel` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    /// Releases only.
    #[default]
    Stable,
    /// Releases and pre-releases (`0.3.0-beta.1`).
    Beta,
}

impl Channel {
    /// The setting value; unknown values fall back to stable.
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("beta") => Self::Beta,
            _ => Self::Stable,
        }
    }

    /// The feed to read when `updates.endpoint` is empty.
    pub fn default_endpoint(self) -> &'static str {
        match self {
            Self::Stable => DEFAULT_ENDPOINT,
            Self::Beta => BETA_ENDPOINT,
        }
    }

    /// The feed for this channel and the `updates.endpoint` setting (empty = default).
    pub fn endpoint(self, setting: Option<&str>) -> String {
        match setting.map(str::trim) {
            Some(url) if !url.is_empty() => url.to_string(),
            _ => self.default_endpoint().to_string(),
        }
    }

    /// Whether a release of this version is offered on this channel.
    pub fn accepts(self, version: &Version) -> bool {
        self == Self::Beta || version.pre.is_empty()
    }
}

/// "Later" on an update offer: the version is not offered again until the next day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Postponed {
    pub version: String,
    /// Unix seconds.
    pub at: u64,
}

/// What Forge remembers between starts (`<config dir>/forge/updates.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdateState {
    /// Unix seconds of the last completed check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check: Option<u64>,
    /// "Skip this version": never offered automatically again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postponed: Option<Postponed>,
}

impl UpdateState {
    /// Read the state file; a missing or broken file is an empty state.
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn record_check(&mut self, now: u64) {
        self.last_check = Some(now);
    }

    pub fn skip(&mut self, version: &str) {
        self.skipped_version = Some(version.to_string());
        self.postponed = None;
    }

    pub fn postpone(&mut self, version: &str, now: u64) {
        self.postponed = Some(Postponed {
            version: version.to_string(),
            at: now,
        });
    }

    /// Offer the skipped version again (Settings | … | Updates).
    pub fn clear_skipped(&mut self) {
        self.skipped_version = None;
    }

    /// Seconds until the next automatic check is due (0 = due now).
    pub fn next_check_in(&self, now: u64) -> u64 {
        match self.last_check {
            // A clock that went backwards makes the check due.
            Some(last) if last <= now => CHECK_INTERVAL_SECS.saturating_sub(now - last),
            _ => 0,
        }
    }
}

/// Everything [`plan`] looks at besides the remembered state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanInput {
    /// Help | Check for Updates… or the settings page: ignore the daily limit and the
    /// `updates.checkOnStartup` setting.
    pub manual: bool,
    /// A debug build (`pnpm tauri dev`) never checks for real updates.
    pub debug_build: bool,
    /// `FORGE_SIMULATE_UPDATE` / `?simulateUpdate`: the version to pretend is available.
    pub simulate: Option<String>,
    /// The build has an updater public key (`FORGE_UPDATER_PUBKEY` at build time).
    pub pubkey_configured: bool,
    /// The `updates.checkOnStartup` setting.
    pub check_on_startup: bool,
    /// Unix seconds.
    pub now: u64,
}

/// Why no check runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "reason")]
pub enum SkipReason {
    /// Debug builds and browser mode never check (simulate instead).
    DevBuild,
    /// The build has no updater public key.
    NotConfigured,
    /// `updates.checkOnStartup` is off.
    Disabled,
    /// Checked less than a day ago.
    CheckedRecently { next_in_secs: u64 },
}

impl SkipReason {
    /// Status line detail on the splash screen.
    pub fn detail(&self) -> &'static str {
        match self {
            Self::DevBuild => "development build",
            Self::NotConfigured => "not configured in this build",
            Self::Disabled => "turned off",
            Self::CheckedRecently { .. } => "checked today",
        }
    }
}

/// What the update step does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckPlan {
    /// Read the feed.
    Check,
    /// Pretend this version is available (never installs).
    Simulate(String),
    Skip(SkipReason),
}

/// Decide whether to check for updates now.
pub fn plan(input: &PlanInput, state: &UpdateState) -> CheckPlan {
    if let Some(version) = input.simulate.as_deref().map(str::trim) {
        if !version.is_empty() {
            return CheckPlan::Simulate(version.trim_start_matches('v').to_string());
        }
    }
    if input.debug_build {
        return CheckPlan::Skip(SkipReason::DevBuild);
    }
    if !input.pubkey_configured {
        return CheckPlan::Skip(SkipReason::NotConfigured);
    }
    if input.manual {
        return CheckPlan::Check;
    }
    if !input.check_on_startup {
        return CheckPlan::Skip(SkipReason::Disabled);
    }
    match state.next_check_in(input.now) {
        0 => CheckPlan::Check,
        next_in_secs => CheckPlan::Skip(SkipReason::CheckedRecently { next_in_secs }),
    }
}

/// What a release in the feed means for this installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Offer it.
    Update,
    /// Not newer than the running version.
    UpToDate,
    /// A pre-release on the stable channel.
    OtherChannel,
    /// The user chose "Skip this version".
    Skipped,
    /// The user chose "Later" less than a day ago.
    Postponed,
}

/// Decide whether `remote` is offered. Manual checks ignore skipped and postponed versions.
pub fn evaluate(
    current: &Version,
    remote: &Version,
    channel: Channel,
    state: &UpdateState,
    manual: bool,
    now: u64,
) -> Verdict {
    if remote <= current {
        return Verdict::UpToDate;
    }
    if !channel.accepts(remote) {
        return Verdict::OtherChannel;
    }
    if manual {
        return Verdict::Update;
    }
    let same = |v: &str| crate::parse_version(v).is_ok_and(|v| &v == remote);
    if state.skipped_version.as_deref().is_some_and(same) {
        return Verdict::Skipped;
    }
    if let Some(p) = &state.postponed {
        if same(&p.version) && now.saturating_sub(p.at) < CHECK_INTERVAL_SECS {
            return Verdict::Postponed;
        }
    }
    Verdict::Update
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn input() -> PlanInput {
        PlanInput {
            manual: false,
            debug_build: false,
            simulate: None,
            pubkey_configured: true,
            check_on_startup: true,
            now: NOW,
        }
    }

    #[test]
    fn checks_at_most_once_a_day() {
        let mut state = UpdateState::default();
        assert_eq!(plan(&input(), &state), CheckPlan::Check);
        state.record_check(NOW - 3600);
        assert_eq!(
            plan(&input(), &state),
            CheckPlan::Skip(SkipReason::CheckedRecently {
                next_in_secs: CHECK_INTERVAL_SECS - 3600
            })
        );
        state.record_check(NOW - CHECK_INTERVAL_SECS);
        assert_eq!(plan(&input(), &state), CheckPlan::Check);
        // A clock that jumped back does not block checks for a day.
        state.record_check(NOW + 3600);
        assert_eq!(plan(&input(), &state), CheckPlan::Check);
    }

    #[test]
    fn manual_checks_ignore_the_daily_limit_and_the_setting() {
        let mut state = UpdateState::default();
        state.record_check(NOW - 10);
        let manual = PlanInput {
            manual: true,
            check_on_startup: false,
            ..input()
        };
        assert_eq!(plan(&manual, &state), CheckPlan::Check);
        let off = PlanInput {
            check_on_startup: false,
            ..input()
        };
        assert_eq!(
            plan(&off, &UpdateState::default()),
            CheckPlan::Skip(SkipReason::Disabled)
        );
    }

    #[test]
    fn dev_builds_and_builds_without_a_key_never_check() {
        let dev = PlanInput {
            debug_build: true,
            manual: true,
            ..input()
        };
        assert_eq!(
            plan(&dev, &UpdateState::default()),
            CheckPlan::Skip(SkipReason::DevBuild)
        );
        let unsigned = PlanInput {
            pubkey_configured: false,
            manual: true,
            ..input()
        };
        assert_eq!(
            plan(&unsigned, &UpdateState::default()),
            CheckPlan::Skip(SkipReason::NotConfigured)
        );
    }

    #[test]
    fn simulation_wins_over_everything() {
        let sim = PlanInput {
            debug_build: true,
            pubkey_configured: false,
            simulate: Some(" v0.2.0".into()),
            ..input()
        };
        assert_eq!(
            plan(&sim, &UpdateState::default()),
            CheckPlan::Simulate("0.2.0".into())
        );
        let empty = PlanInput {
            simulate: Some("".into()),
            ..input()
        };
        assert_eq!(plan(&empty, &UpdateState::default()), CheckPlan::Check);
    }

    #[test]
    fn only_newer_versions_are_updates() {
        let s = UpdateState::default();
        let ch = Channel::Stable;
        assert_eq!(
            evaluate(&v("0.1.0"), &v("0.2.0"), ch, &s, false, NOW),
            Verdict::Update
        );
        assert_eq!(
            evaluate(&v("0.2.0"), &v("0.2.0"), ch, &s, false, NOW),
            Verdict::UpToDate
        );
        assert_eq!(
            evaluate(&v("0.2.0"), &v("0.1.9"), ch, &s, true, NOW),
            Verdict::UpToDate
        );
        // 0.2.0-beta.1 < 0.2.0 in semver.
        assert_eq!(
            evaluate(&v("0.2.0-beta.1"), &v("0.2.0"), ch, &s, false, NOW),
            Verdict::Update
        );
    }

    #[test]
    fn pre_releases_only_on_the_beta_channel() {
        let s = UpdateState::default();
        let beta = v("0.3.0-beta.1");
        assert_eq!(
            evaluate(&v("0.2.0"), &beta, Channel::Stable, &s, true, NOW),
            Verdict::OtherChannel
        );
        assert_eq!(
            evaluate(&v("0.2.0"), &beta, Channel::Beta, &s, false, NOW),
            Verdict::Update
        );
    }

    #[test]
    fn skipped_and_postponed_versions_wait_for_a_manual_check() {
        let mut s = UpdateState::default();
        s.skip("v0.2.0");
        let ch = Channel::Stable;
        assert_eq!(
            evaluate(&v("0.1.0"), &v("0.2.0"), ch, &s, false, NOW),
            Verdict::Skipped
        );
        assert_eq!(
            evaluate(&v("0.1.0"), &v("0.2.1"), ch, &s, false, NOW),
            Verdict::Update
        );
        assert_eq!(
            evaluate(&v("0.1.0"), &v("0.2.0"), ch, &s, true, NOW),
            Verdict::Update
        );

        let mut s = UpdateState::default();
        s.postpone("0.2.0", NOW - 60);
        assert_eq!(
            evaluate(&v("0.1.0"), &v("0.2.0"), ch, &s, false, NOW),
            Verdict::Postponed
        );
        assert_eq!(
            evaluate(
                &v("0.1.0"),
                &v("0.2.0"),
                ch,
                &s,
                false,
                NOW + CHECK_INTERVAL_SECS
            ),
            Verdict::Update
        );
        // Skipping replaces a postponement.
        s.skip("0.2.0");
        assert!(s.postponed.is_none());
        s.clear_skipped();
        assert_eq!(s, UpdateState::default());
    }

    #[test]
    fn channels_pick_their_feed_unless_overridden() {
        assert_eq!(Channel::from_setting(Some("beta")), Channel::Beta);
        assert_eq!(Channel::from_setting(Some("nightly")), Channel::Stable);
        assert_eq!(Channel::Stable.endpoint(None), DEFAULT_ENDPOINT);
        assert_eq!(Channel::Beta.endpoint(Some("  ")), BETA_ENDPOINT);
        assert_eq!(
            Channel::Stable.endpoint(Some(" https://updates.example.test/latest.json ")),
            "https://updates.example.test/latest.json"
        );
    }

    #[test]
    fn state_round_trips_and_tolerates_garbage() {
        let mut s = UpdateState::default();
        s.record_check(NOW);
        s.postpone("0.2.0", NOW);
        assert_eq!(UpdateState::from_json(&s.to_json()), s);
        assert_eq!(UpdateState::from_json("not json"), UpdateState::default());
        assert_eq!(UpdateState::default().to_json(), "{}");
    }
}
