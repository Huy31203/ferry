//! Short eased slides for floating windows (the stack and the drop zone make
//! room for the shelf with one). macOS only: elsewhere the window just moves.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tauri::WebviewWindow;

/// Matches the shelf page's rise animation.
const MS: u64 = 160;
const STEPS: u64 = 10;

/// The newest slide per window label; older ones stop.
static LATEST: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

fn begin(label: &str) -> u64 {
    let mut map = LATEST.lock().unwrap();
    let n = map
        .get_or_insert_with(HashMap::new)
        .entry(label.to_string())
        .or_insert(0);
    *n += 1;
    *n
}

fn current(label: &str) -> u64 {
    LATEST
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(label).copied())
        .unwrap_or(0)
}

/// Move `window` to `target` (physical pixels) over a moment.
pub fn to(window: &WebviewWindow, scale: f64, target: (i32, i32)) {
    #[cfg(target_os = "macos")]
    {
        use tauri::LogicalPosition;
        let Ok(from) = window.outer_position() else {
            return;
        };
        let (fx, fy) = (f64::from(from.x) / scale, f64::from(from.y) / scale);
        let (tx, ty) = (f64::from(target.0) / scale, f64::from(target.1) / scale);
        if (fx - tx).abs() < 1.0 && (fy - ty).abs() < 1.0 {
            let _ = window.set_position(LogicalPosition::new(tx, ty));
            return;
        }
        let generation = begin(window.label());
        let window = window.clone();
        let w0 = window.clone();
        std::thread::spawn(move || {
            for step in 1..=STEPS {
                std::thread::sleep(Duration::from_millis(MS / STEPS));
                if current(w0.label()) != generation {
                    return;
                }
                let t = step as f64 / STEPS as f64;
                let eased = 1.0 - (1.0 - t).powi(3);
                let (x, y) = (fx + (tx - fx) * eased, fy + (ty - fy) * eased);
                let w = window.clone();
                let _ = window.run_on_main_thread(move || {
                    let _ = w.set_position(LogicalPosition::new(x, y));
                });
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (scale, Duration::ZERO, begin, current);
        let _ = window.set_position(tauri::PhysicalPosition::new(target.0, target.1));
    }
}
