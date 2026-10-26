//! The pipeline editor window (stream F): one window, shown on demand, that
//! the Settings pane, the card menu and the zones open at a pipeline or a tab.

use ferry_core::i18n::t;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const PIPELINES: &str = "pipelines";

#[derive(Serialize, Clone)]
struct Focus {
    pipeline: Option<u32>,
    tab: Option<String>,
}

/// The page address with only the parameters that are set.
fn url(pipeline: Option<u32>, tab: Option<&str>) -> String {
    let mut query = Vec::new();
    if let Some(pipeline) = pipeline {
        query.push(format!("pipeline={pipeline}"));
    }
    if let Some(tab) = tab {
        query.push(format!("tab={tab}"));
    }
    if query.is_empty() {
        "pipelines.html".into()
    } else {
        format!("pipelines.html?{}", query.join("&"))
    }
}

/// Show the editor, creating it the first time. An open one is sent
/// `pipe:focus` to go to `pipeline` and `tab`.
pub fn open_editor(app: &AppHandle, pipeline: Option<u32>, tab: Option<&str>) {
    if let Some(window) = app.get_webview_window(PIPELINES) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let focus = Focus {
            pipeline,
            tab: tab.map(str::to_owned),
        };
        let _ = window.emit("pipe:focus", focus);
        return;
    }
    let built =
        WebviewWindowBuilder::new(app, PIPELINES, WebviewUrl::App(url(pipeline, tab).into()))
            .title(t("pipe.window_title"))
            .inner_size(1000.0, 680.0)
            .min_inner_size(860.0, 560.0)
            .center()
            .focused(true)
            .build();
    match built {
        // A tray app is never frontmost, so a new window opens behind other apps without this.
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("ferry: could not open the pipeline editor: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_has_only_the_parameters_that_are_set() {
        assert_eq!(url(None, None), "pipelines.html");
        assert_eq!(url(Some(3), None), "pipelines.html?pipeline=3");
        assert_eq!(url(None, Some("zones")), "pipelines.html?tab=zones");
        assert_eq!(
            url(Some(3), Some("zones")),
            "pipelines.html?pipeline=3&tab=zones"
        );
    }
}
