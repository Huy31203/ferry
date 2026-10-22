import type { Settings } from './settings';

/** The defaults of ferry_core::settings::Settings, for tests. */
export const sample: Settings = {
  version: 1,
  general: {
    launch_at_login: true,
    start_state: 'running',
    show_dock_icon: false,
    show_in_screenshots: false,
    tray_style: 'default',
    language: 'en',
  },
  clipboard: {
    enabled: true,
    skip_multiple_files: true,
    max_size_mb: 50,
    ignored_apps: [],
    accumulate: false,
    copy_paths: false,
    file_template: '%f',
  },
  images: {
    compression: 80,
    adaptive: false,
    switch_png_jpeg: false,
    strip_metadata: true,
    preserve_dates: true,
    ignored_extensions: [],
    max_size_mb: 50,
  },
  floating: {
    enabled: true,
    corner: 'bottom_right',
    card_size: 'default',
    auto_hide_secs: 6,
    follow_cursor: true,
  },
  backups: { retention_days: 7 },
  hotkeys: { optimise: 'Ctrl+Shift+C', aggressive: 'Ctrl+Shift+A', cycle_state: 'Ctrl+Shift+P' },
};
