//! The first-run screen (onboarding spec): opens until `general.onboarded`.

use std::sync::Mutex;

use ferry_core::cancel::CancelToken;
use ferry_core::i18n::t;
use ferry_core::optimise::Encoder;
use ferry_core::video::{bench, hardware};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::state::AppState;

pub const ONBOARDING: &str = "onboarding";

static MEASURING: Mutex<Option<CancelToken>> = Mutex::new(None);

#[derive(Clone, Serialize)]
struct Measured {
    encoder: &'static str,
    label: String,
}

pub fn open_if_needed(app: &AppHandle) {
    let done = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .general
        .onboarded;
    if done || app.get_webview_window(ONBOARDING).is_some() {
        return;
    }
    let built =
        WebviewWindowBuilder::new(app, ONBOARDING, WebviewUrl::App("onboarding.html".into()))
            .title(t("onboarding.title"))
            .inner_size(520.0, 560.0)
            .resizable(false)
            .center()
            .build();
    match built {
        // A tray app is never frontmost, so a new window opens behind other apps without this.
        Ok(window) => {
            let handle = app.clone();
            window.on_window_event(move |event| {
                // The close button saves like Done: the page saves, then destroys the window.
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    stop();
                    let _ = handle.emit_to(ONBOARDING, "onboarding:closing", ());
                }
            });
            let _ = window.set_focus();
        }
        Err(e) => tracing::warn!("could not open onboarding: {e}"),
    }
}

fn stop() {
    if let Some(token) = MEASURING.lock().unwrap().take() {
        token.cancel();
    }
}

fn id(encoder: Encoder) -> &'static str {
    match encoder {
        Encoder::Fast => "fast",
        Encoder::Small => "small",
        Encoder::VisuallyLossless => "visually_lossless",
    }
}

/// Time each encoder off the main thread; each result is an `onboarding:measured` event.
#[tauri::command]
pub fn onboarding_measure(app: AppHandle) {
    stop();
    let token = CancelToken::new();
    *MEASURING.lock().unwrap() = Some(token.clone());
    let video = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .video
        .clone();
    std::thread::spawn(move || {
        // Waits for the startup probe, so Fast is timed on the hardware encoder.
        hardware::probe_hardware();
        bench::measure_all(&video, &token, &mut |encoder, secs| {
            let measured = Measured {
                encoder: id(encoder),
                label: bench::label(secs),
            };
            let _ = app.emit_to(ONBOARDING, "onboarding:measured", measured);
        });
    });
}

/// Done was pressed and the settings are saved: stop measuring and close.
#[tauri::command]
pub fn onboarding_closed(app: AppHandle) {
    stop();
    if let Some(w) = app.get_webview_window(ONBOARDING) {
        let _ = w.destroy();
    }
}
