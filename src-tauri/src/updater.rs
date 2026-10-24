//! Self-update from signed GitHub Releases (stream G). `tauri-plugin-updater`
//! is driven from Rust only: a state machine here emits `svc:update` to
//! Settings, the stack pill and the tray. A failed automatic check stays
//! quiet; it shows only in Settings ▸ About and updates.

use std::mem::discriminant;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ferry_core::i18n::{t, tf};
use serde::Serialize;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_updater::{Update, UpdaterExt};
use tracing::{info, warn};

use crate::state::AppState;
use crate::{prefs, svc_commands, tray};

pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(60);
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
pub const RELEASES: &str = "https://github.com/Huy31203/ferry/releases/tag/v";
/// Download progress is announced at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
const TRAY_ID: &str = "svc:update";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Check,
    Download,
    Install,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: String,
    },
    Downloading {
        version: String,
        received: u64,
        total: Option<u64>,
    },
    Ready {
        version: String,
        busy: usize,
    },
    Installing {
        version: String,
    },
    Failed {
        phase: Phase,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateView {
    pub current: String,
    pub state: UpdateState,
    pub skipped: bool,
    pub pill: bool,
    pub release_url: Option<String>,
}

/// The version an update state is about, if it is about one.
fn offered(state: &UpdateState) -> Option<&str> {
    match state {
        UpdateState::Available { version, .. }
        | UpdateState::Downloading { version, .. }
        | UpdateState::Ready { version, .. }
        | UpdateState::Installing { version } => Some(version),
        _ => None,
    }
}

pub fn is_skipped(version: &str, skipped_version: &str) -> bool {
    !skipped_version.is_empty() && version == skipped_version
}

/// The "Update available" pill under the cards: an offered update the user has
/// not skipped or put off, or one on its way in.
pub fn pill_visible(state: &UpdateState, skipped: bool, dismissed: bool, setting: bool) -> bool {
    if !setting || dismissed {
        return false;
    }
    match state {
        UpdateState::Available { .. } => !skipped,
        UpdateState::Downloading { .. } | UpdateState::Ready { .. } => true,
        _ => false,
    }
}

/// Whether the quiet automatic check is due: a minute after start, then every 6 hours.
pub fn check_due(since_start: Duration, since_last: Option<Duration>, auto: bool) -> bool {
    auto && since_start >= FIRST_CHECK_AFTER && since_last.is_none_or(|last| last >= CHECK_EVERY)
}

/// The first line of `reason`, trimmed, at most 160 characters ending in "…".
pub fn one_line(reason: &str) -> String {
    let line = reason.lines().next().unwrap_or_default().trim();
    if line.chars().count() <= 160 {
        line.to_string()
    } else {
        let head: String = line.chars().take(159).collect();
        format!("{head}…")
    }
}

pub fn release_url(version: &str) -> String {
    format!("{RELEASES}{version}")
}

/// The tray item's text and whether it can be clicked.
pub fn tray_label(state: &UpdateState, skipped: bool) -> (String, bool) {
    match state {
        UpdateState::Available { version, .. } | UpdateState::Ready { version, .. } if !skipped => {
            (tf("svc.tray.update_to", &[("version", version)]), true)
        }
        UpdateState::Downloading { .. } | UpdateState::Installing { .. } => {
            (t("svc.tray.downloading"), false)
        }
        _ => (t("svc.tray.check"), true),
    }
}

struct Updates {
    started: Instant,
    inner: Mutex<Inner>,
}

struct Inner {
    state: UpdateState,
    /// The update the last check found.
    update: Option<Update>,
    /// The downloaded package, once it is whole and verified.
    bytes: Option<Vec<u8>>,
    /// The version whose pill the user put off with "Later".
    dismissed: Option<String>,
    last_check: Option<Instant>,
}

/// Manage the state; release builds also start the quiet scheduler. Debug
/// builds never check on their own, as they never touch the login item.
pub fn init(app: &AppHandle) {
    app.manage(Updates {
        started: Instant::now(),
        inner: Mutex::new(Inner {
            state: UpdateState::Idle,
            update: None,
            bytes: None,
            dismissed: None,
            last_check: None,
        }),
    });
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_AFTER);
        loop {
            if due(&app) {
                let _ = tauri::async_runtime::block_on(check(app.clone(), false));
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

fn due(app: &AppHandle) -> bool {
    let auto = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .svc
        .updates
        .auto_check;
    let updates = app.state::<Updates>();
    let inner = updates.inner.lock().unwrap();
    let quiet = matches!(
        inner.state,
        UpdateState::Idle | UpdateState::UpToDate | UpdateState::Failed { .. }
    );
    quiet
        && check_due(
            updates.started.elapsed(),
            inner.last_check.map(|at| at.elapsed()),
            auto,
        )
}

pub fn view(app: &AppHandle) -> UpdateView {
    // The tray is built before `init`, and asks for this.
    let (state, dismissed) = match app.try_state::<Updates>() {
        Some(updates) => {
            let inner = updates.inner.lock().unwrap();
            let dismissed =
                inner.dismissed.is_some() && inner.dismissed.as_deref() == offered(&inner.state);
            (inner.state.clone(), dismissed)
        }
        None => (UpdateState::Idle, false),
    };
    let settings = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .svc
        .updates
        .clone();
    let skipped = offered(&state).is_some_and(|v| is_skipped(v, &settings.skipped_version));
    UpdateView {
        current: env!("CARGO_PKG_VERSION").into(),
        pill: pill_visible(&state, skipped, dismissed, settings.pill),
        release_url: offered(&state).map(release_url),
        skipped,
        state,
    }
}

/// Change the state, tell every window, and rebuild the tray when the kind of
/// state changed. No lock is held while announcing.
fn set(app: &AppHandle, change: impl FnOnce(&mut Inner)) {
    let kind_changed = {
        let updates = app.state::<Updates>();
        let mut inner = updates.inner.lock().unwrap();
        let before = discriminant(&inner.state);
        change(&mut inner);
        discriminant(&inner.state) != before
    };
    announce(app, kind_changed);
}

fn announce(app: &AppHandle, rebuild_tray: bool) {
    let _ = app.emit("svc:update", view(app));
    if rebuild_tray {
        tray::rebuild(app);
    }
}

/// Ask GitHub for a newer version. Does nothing while a check, download or
/// install is under way, or while a downloaded update waits. Only a manual
/// caller gets an `Err`; an automatic failure shows in Settings alone.
pub async fn check(app: AppHandle, manual: bool) -> Result<(), String> {
    let proceed = {
        let updates = app.state::<Updates>();
        let mut inner = updates.inner.lock().unwrap();
        let busy = matches!(
            inner.state,
            UpdateState::Checking
                | UpdateState::Downloading { .. }
                | UpdateState::Installing { .. }
                | UpdateState::Ready { .. }
        );
        if !busy {
            inner.last_check = Some(Instant::now());
        }
        !busy
    };
    if !proceed {
        return Ok(());
    }
    set(&app, |inner| inner.state = UpdateState::Checking);
    info!(manual, "checking for updates");
    let result = match app.updater_builder().timeout(CHECK_TIMEOUT).build() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    match result {
        Ok(Some(update)) => {
            info!(version = %update.version, "update available");
            let state = UpdateState::Available {
                version: update.version.clone(),
                notes: update.body.clone().unwrap_or_default(),
            };
            set(&app, |inner| {
                inner.state = state;
                inner.update = Some(update);
                inner.bytes = None;
            });
            Ok(())
        }
        Ok(None) => {
            info!("Ferry is up to date");
            set(&app, |inner| {
                inner.state = UpdateState::UpToDate;
                inner.update = None;
                inner.bytes = None;
            });
            Ok(())
        }
        Err(e) => {
            let reason = one_line(&e.to_string());
            warn!(manual, %reason, "update check failed");
            set(&app, |inner| {
                inner.state = UpdateState::Failed {
                    phase: Phase::Check,
                    reason: reason.clone(),
                };
            });
            if manual { Err(reason) } else { Ok(()) }
        }
    }
}

fn busy_jobs(app: &AppHandle) -> usize {
    app.state::<AppState>()
        .jobs
        .lock()
        .unwrap()
        .values()
        .filter(|entry| entry.is_busy())
        .count()
}

/// Download the offered update, then install it at once if no job is busy, or
/// wait as "Ready" with the busy count. From Ready it installs whatever is busy.
pub async fn install(app: AppHandle) -> Result<(), String> {
    let (update, bytes, from_ready) = {
        let updates = app.state::<Updates>();
        let mut inner = updates.inner.lock().unwrap();
        let allowed = matches!(
            inner.state,
            UpdateState::Available { .. }
                | UpdateState::Ready { .. }
                | UpdateState::Failed {
                    phase: Phase::Download | Phase::Install,
                    ..
                }
        );
        if !allowed {
            return Ok(());
        }
        let Some(update) = inner.update.clone() else {
            return Err("no update is waiting".into());
        };
        inner.dismissed = None;
        (
            update,
            inner.bytes.clone(),
            matches!(inner.state, UpdateState::Ready { .. }),
        )
    };
    let version = update.version.clone();
    let bytes = match bytes {
        Some(bytes) => bytes,
        None => {
            set(&app, |inner| {
                inner.state = UpdateState::Downloading {
                    version: version.clone(),
                    received: 0,
                    total: None,
                };
            });
            let mut received = 0_u64;
            let mut announced = Instant::now();
            let result = update
                .download(
                    |chunk, total| {
                        received += chunk as u64;
                        if announced.elapsed() >= PROGRESS_EVERY {
                            announced = Instant::now();
                            set(&app, |inner| {
                                inner.state = UpdateState::Downloading {
                                    version: version.clone(),
                                    received,
                                    total,
                                };
                            });
                        }
                    },
                    || {},
                )
                .await;
            match result {
                Ok(bytes) => bytes,
                Err(e) => {
                    let reason = one_line(&e.to_string());
                    warn!(%reason, "update download failed");
                    set(&app, |inner| {
                        inner.state = UpdateState::Failed {
                            phase: Phase::Download,
                            reason: reason.clone(),
                        };
                    });
                    return Err(reason);
                }
            }
        }
    };
    let busy = busy_jobs(&app);
    if busy > 0 && !from_ready {
        info!(busy, "update downloaded, waiting for the user");
        set(&app, |inner| {
            inner.bytes = Some(bytes);
            inner.state = UpdateState::Ready {
                version: version.clone(),
                busy,
            };
        });
        return Ok(());
    }
    set(&app, |inner| {
        inner.bytes = Some(bytes.clone());
        inner.state = UpdateState::Installing {
            version: version.clone(),
        };
    });
    info!(%version, "installing the update");
    let installed = tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| e.to_string())?;
    match installed {
        // Windows' `install` launches the installer and exits the process itself.
        Ok(()) => {
            info!("update installed, restarting");
            app.request_restart();
            Ok(())
        }
        Err(e) => {
            let reason = one_line(&e.to_string());
            warn!(%reason, "update install failed");
            set(&app, |inner| {
                inner.state = UpdateState::Failed {
                    phase: Phase::Install,
                    reason: reason.clone(),
                };
            });
            Err(reason)
        }
    }
}

/// Put the pill off for this run ("Later").
pub fn later(app: &AppHandle) {
    set(app, |inner| {
        inner.dismissed = offered(&inner.state).map(String::from);
    });
}

/// Stop offering the version on offer: `svc.updates.skipped_version`.
pub fn skip(app: &AppHandle) -> Result<(), String> {
    let version = match view(app).state {
        UpdateState::Available { version, .. } | UpdateState::Ready { version, .. } => version,
        _ => return Err("no update is on offer".into()),
    };
    prefs::update(app, |s| s.svc.updates.skipped_version = version).map(|_| ())
}

pub fn tray_item(app: &AppHandle) -> tauri::Result<MenuItem<Wry>> {
    let view = view(app);
    let (label, enabled) = tray_label(&view.state, view.skipped);
    MenuItem::with_id(app, TRAY_ID, label, enabled, None::<&str>)
}

/// The tray item: open About and updates, and check unless something is under
/// way or an update is already on offer.
pub fn tray_clicked(app: &AppHandle) {
    svc_commands::open_settings_at(app, "about", None);
    let view = view(app);
    let offered_now = matches!(
        view.state,
        UpdateState::Available { .. } | UpdateState::Ready { .. }
    ) && !view.skipped;
    let busy = matches!(
        view.state,
        UpdateState::Checking | UpdateState::Downloading { .. } | UpdateState::Installing { .. }
    );
    if !busy && !offered_now {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = check(app, true).await;
        });
    }
}

/// The pill setting or the skipped version changed: say so again.
pub fn settings_changed(app: &AppHandle) {
    announce(app, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available(version: &str) -> UpdateState {
        UpdateState::Available {
            version: version.into(),
            notes: String::new(),
        }
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn the_pill_shows_only_for_an_offered_update() {
        let on = |state: &UpdateState, skipped, dismissed, setting| {
            pill_visible(state, skipped, dismissed, setting)
        };
        let offer = available("0.3.0");
        assert!(on(&offer, false, false, true));
        assert!(!on(&offer, true, false, true));
        assert!(!on(&offer, false, true, true));
        assert!(!on(&offer, false, false, false));
        let downloading = UpdateState::Downloading {
            version: "0.3.0".into(),
            received: 1,
            total: None,
        };
        let ready = UpdateState::Ready {
            version: "0.3.0".into(),
            busy: 2,
        };
        assert!(on(&downloading, false, false, true));
        assert!(on(&ready, false, false, true));
        for state in [
            UpdateState::Idle,
            UpdateState::Checking,
            UpdateState::UpToDate,
            UpdateState::Installing {
                version: "0.3.0".into(),
            },
            UpdateState::Failed {
                phase: Phase::Download,
                reason: "x".into(),
            },
        ] {
            assert!(!on(&state, false, false, true), "{state:?}");
        }
    }

    #[test]
    fn a_skipped_version_stays_quiet() {
        assert!(is_skipped("0.3.0", "0.3.0"));
        assert!(!is_skipped("0.3.1", "0.3.0"));
        assert!(!is_skipped("0.3.0", ""));
        assert_eq!(tray_label(&available("0.3.0"), true).0, t("svc.tray.check"));
        assert_eq!(
            tray_label(&available("0.3.0"), false).0,
            tf("svc.tray.update_to", &[("version", "0.3.0")])
        );
    }

    #[test]
    fn checks_wait_a_minute_then_run_every_six_hours() {
        let hour = 3600;
        assert!(!check_due(secs(30), None, true));
        assert!(check_due(secs(61), None, true));
        assert!(!check_due(secs(7 * hour), Some(secs(5 * hour)), true));
        assert!(check_due(secs(7 * hour), Some(secs(6 * hour + 1)), true));
        assert!(!check_due(secs(7 * hour), None, false));
        assert!(!check_due(secs(61), None, false));
    }

    #[test]
    fn automatic_failures_stay_quiet() {
        let failed = UpdateState::Failed {
            phase: Phase::Check,
            reason: "offline".into(),
        };
        assert_eq!(tray_label(&failed, false), (t("svc.tray.check"), true));
        assert!(!pill_visible(&failed, false, false, true));
    }

    #[test]
    fn one_line_keeps_the_first_line_only() {
        assert_eq!(
            one_line("error sending request\ncaused by: dns"),
            "error sending request"
        );
        let long = one_line(&"x".repeat(300));
        assert_eq!(long.chars().count(), 160);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn release_urls_point_at_the_tag() {
        assert_eq!(
            release_url("0.3.0"),
            "https://github.com/Huy31203/ferry/releases/tag/v0.3.0"
        );
    }

    #[test]
    fn the_tray_item_is_disabled_while_busy() {
        let downloading = UpdateState::Downloading {
            version: "0.3.0".into(),
            received: 0,
            total: Some(10),
        };
        let installing = UpdateState::Installing {
            version: "0.3.0".into(),
        };
        for state in [downloading, installing] {
            assert_eq!(
                tray_label(&state, false),
                (t("svc.tray.downloading"), false)
            );
        }
    }
}
