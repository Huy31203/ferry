// sE pdf-formats: the PDF settings the card tools read (DPI defaults, crop devices).
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { PdfConfig } from './pdf';
import type { Settings } from './settings';

export const pdfConfig = $state<PdfConfig>({ dpi: 150, dpi_aggressive: 100, extract_dpi: 150, devices: [] });

let following = false;

/** Fill `pdfConfig` from the app, and keep it current when settings change. */
export async function loadPdfConfig(): Promise<void> {
  if (!following) {
    following = true;
    listen<Settings>('settings:changed', (event) => {
      const { dpi, dpi_aggressive, extract_dpi, devices } = event.payload.pdf;
      Object.assign(pdfConfig, { dpi, dpi_aggressive, extract_dpi, devices });
    }).catch((e) => console.warn('settings:changed', e));
  }
  Object.assign(pdfConfig, await invoke<PdfConfig>('pdf_config'));
}
