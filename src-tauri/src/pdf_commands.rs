//! Commands the PDF card tools call (sE pdf-formats).

use ferry_core::settings::Device;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// What the PDF tools read from settings.
#[derive(Serialize)]
pub struct PdfConfig {
    dpi: u16,
    dpi_aggressive: u16,
    extract_dpi: u16,
    devices: Vec<Device>,
}

#[tauri::command]
pub fn pdf_config(app: AppHandle) -> PdfConfig {
    let pdf = app.state::<AppState>().settings.read().unwrap().pdf.clone();
    PdfConfig {
        dpi: pdf.dpi,
        dpi_aggressive: pdf.dpi_aggressive,
        extract_dpi: pdf.extract_dpi,
        devices: pdf.devices,
    }
}
