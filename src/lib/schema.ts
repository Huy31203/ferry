import type { StringKey } from './i18n';
import type { HotkeyId } from './svc';

export type PaneId =
  | 'general'
  | 'clipboard'
  | 'images'
  | 'files'
  | 'floating'
  | 'hotkeys' // svc
  | 'about'; // svc

export const panes: { id: PaneId; label: StringKey }[] = [
  { id: 'general', label: 'pane.general' },
  { id: 'clipboard', label: 'pane.clipboard' },
  { id: 'images', label: 'pane.images' },
  { id: 'files', label: 'pane.files' },
  { id: 'floating', label: 'pane.floating' },
  { id: 'hotkeys', label: 'svc.pane.hotkeys' }, // svc
  { id: 'about', label: 'svc.pane.about' }, // svc
];

export type Control =
  | { kind: 'switch' }
  | { kind: 'slider'; min: number; max: number }
  | { kind: 'number'; min: number; max: number; unit: StringKey }
  | { kind: 'select'; options: { value: string; label: StringKey }[] }
  | { kind: 'template' }
  | { kind: 'extensions' }
  | { kind: 'apps' }
  | { kind: 'tray_style' }
  | { kind: 'output'; converted: boolean }
  | { kind: 'sizes' }
  | { kind: 'action'; action: 'open_backups' | 'open_work' | 'force_clean'; button: StringKey }
  // svc (stream G)
  | { kind: 'hotkey'; action: HotkeyId }
  | { kind: 'rail' }
  | { kind: 'sync' }
  | { kind: 'update' };

export interface Setting {
  id: string;
  pane: PaneId;
  /** A heading shown above the first row of a run of rows with the same group. */
  group?: StringKey;
  label: StringKey;
  help?: StringKey;
  /** Dotted path into Settings; actions have none. */
  path?: string;
  control: Control;
  /** Path of the switch that must be on for this row to work. */
  depends_on?: string;
  mac_only?: boolean;
  /** svc: the control sits under the text, full width. */
  wide?: boolean;
}

const SIZE_UNIT: StringKey = 'unit.mb';

const all: Setting[] = [
  {
    id: 'launch_at_login',
    pane: 'general',
    label: 'general.launch_at_login',
    help: 'general.launch_at_login.help',
    path: 'general.launch_at_login',
    control: { kind: 'switch' },
  },
  {
    id: 'start_state',
    pane: 'general',
    label: 'general.start_state',
    help: 'general.start_state.help',
    path: 'general.start_state',
    control: {
      kind: 'select',
      options: [
        { value: 'running', label: 'tray.state.running' },
        { value: 'paused', label: 'tray.state.paused' },
        { value: 'stopped', label: 'tray.state.stopped' },
      ],
    },
  },
  {
    id: 'show_dock_icon',
    pane: 'general',
    label: 'general.show_dock_icon',
    help: 'general.show_dock_icon.help',
    path: 'general.show_dock_icon',
    control: { kind: 'switch' },
    mac_only: true,
  },
  {
    id: 'show_in_screenshots',
    pane: 'general',
    label: 'general.show_in_screenshots',
    help: 'general.show_in_screenshots.help',
    path: 'general.show_in_screenshots',
    control: { kind: 'switch' },
  },
  {
    id: 'tray_style',
    pane: 'general',
    label: 'general.tray_style',
    help: 'general.tray_style.help',
    path: 'general.tray_style',
    control: { kind: 'tray_style' },
    mac_only: true,
  },
  {
    id: 'language',
    pane: 'general',
    label: 'general.language',
    help: 'general.language.help',
    path: 'general.language',
    control: { kind: 'select', options: [{ value: 'en', label: 'language.en' }] },
  },
  {
    id: 'retention_days',
    pane: 'general',
    group: 'group.storage',
    label: 'general.retention',
    help: 'general.retention.help',
    path: 'backups.retention_days',
    control: { kind: 'number', min: 1, max: 3650, unit: 'unit.days' },
  },
  {
    id: 'open_backups',
    pane: 'general',
    group: 'group.storage',
    label: 'general.open_backups',
    control: { kind: 'action', action: 'open_backups', button: 'action.open' },
  },
  {
    id: 'open_work',
    pane: 'general',
    group: 'group.storage',
    label: 'general.open_work',
    control: { kind: 'action', action: 'open_work', button: 'action.open' },
  },
  {
    id: 'force_clean',
    pane: 'general',
    group: 'group.storage',
    label: 'general.force_clean',
    help: 'general.force_clean.help',
    control: { kind: 'action', action: 'force_clean', button: 'action.clean' },
  },
  {
    id: 'svc_sync',
    pane: 'general',
    group: 'svc.group.sync',
    label: 'svc.sync.enabled',
    help: 'svc.sync.enabled.help',
    path: 'svc.sync.enabled',
    control: { kind: 'sync' },
    wide: true,
  },

  {
    id: 'clipboard_enabled',
    pane: 'clipboard',
    label: 'clipboard.enabled',
    help: 'clipboard.enabled.help',
    path: 'clipboard.enabled',
    control: { kind: 'switch' },
  },
  {
    id: 'skip_multiple_files',
    pane: 'clipboard',
    label: 'clipboard.skip_multiple_files',
    help: 'clipboard.skip_multiple_files.help',
    path: 'clipboard.skip_multiple_files',
    control: { kind: 'switch' },
    depends_on: 'clipboard.enabled',
  },
  {
    id: 'clipboard_max_size',
    pane: 'clipboard',
    label: 'clipboard.max_size',
    help: 'clipboard.max_size.help',
    path: 'clipboard.max_size_mb',
    control: { kind: 'number', min: 1, max: 4096, unit: SIZE_UNIT },
    depends_on: 'clipboard.enabled',
  },
  {
    id: 'ignored_apps',
    pane: 'clipboard',
    label: 'clipboard.ignored_apps',
    help: 'clipboard.ignored_apps.help',
    path: 'clipboard.ignored_apps',
    control: { kind: 'apps' },
    depends_on: 'clipboard.enabled',
  },
  {
    id: 'accumulate',
    pane: 'clipboard',
    label: 'clipboard.accumulate',
    help: 'clipboard.accumulate.help',
    path: 'clipboard.accumulate',
    control: { kind: 'switch' },
    depends_on: 'clipboard.enabled',
  },
  {
    id: 'copy_paths',
    pane: 'clipboard',
    label: 'clipboard.copy_paths',
    help: 'clipboard.copy_paths.help',
    path: 'clipboard.copy_paths',
    control: { kind: 'switch' },
    depends_on: 'clipboard.enabled',
  },
  {
    id: 'file_template',
    pane: 'clipboard',
    label: 'clipboard.file_template',
    help: 'clipboard.file_template.help',
    path: 'clipboard.file_template',
    control: { kind: 'template' },
    depends_on: 'clipboard.enabled',
  },

  {
    id: 'compression',
    pane: 'images',
    label: 'images.compression',
    help: 'images.compression.help',
    path: 'images.compression',
    control: { kind: 'slider', min: 5, max: 100 },
  },
  {
    id: 'adaptive',
    pane: 'images',
    label: 'images.adaptive',
    help: 'images.adaptive.help',
    path: 'images.adaptive',
    control: { kind: 'switch' },
  },
  {
    id: 'switch_png_jpeg',
    pane: 'images',
    label: 'images.switch_png_jpeg',
    help: 'images.switch_png_jpeg.help',
    path: 'images.switch_png_jpeg',
    control: { kind: 'switch' },
  },
  {
    id: 'strip_metadata',
    pane: 'images',
    label: 'images.strip_metadata',
    help: 'images.strip_metadata.help',
    path: 'images.strip_metadata',
    control: { kind: 'switch' },
  },
  {
    id: 'preserve_dates',
    pane: 'images',
    label: 'images.preserve_dates',
    help: 'images.preserve_dates.help',
    path: 'images.preserve_dates',
    control: { kind: 'switch' },
  },
  {
    id: 'ignored_extensions',
    pane: 'images',
    label: 'images.ignored_extensions',
    help: 'images.ignored_extensions.help',
    path: 'images.ignored_extensions',
    control: { kind: 'extensions' },
  },
  {
    id: 'images_max_size',
    pane: 'images',
    label: 'images.max_size',
    help: 'images.max_size.help',
    path: 'images.max_size_mb',
    control: { kind: 'number', min: 1, max: 4096, unit: SIZE_UNIT },
  },
  {
    id: 'crop_sizes',
    pane: 'images',
    label: 'images.crop_sizes',
    help: 'images.crop_sizes.help',
    path: 'images.crop_sizes',
    control: { kind: 'sizes' },
  },

  {
    id: 'output_optimised',
    pane: 'files',
    group: 'group.images',
    label: 'output.optimised',
    help: 'output.optimised.help',
    path: 'images.output.optimised',
    control: { kind: 'output', converted: false },
  },
  {
    id: 'output_converted',
    pane: 'files',
    group: 'group.images',
    label: 'output.converted',
    help: 'output.converted.help',
    path: 'images.output.converted',
    control: { kind: 'output', converted: true },
  },

  {
    id: 'floating_enabled',
    pane: 'floating',
    label: 'floating.enabled',
    help: 'floating.enabled.help',
    path: 'floating.enabled',
    control: { kind: 'switch' },
  },
  {
    id: 'corner',
    pane: 'floating',
    label: 'floating.corner',
    help: 'floating.corner.help',
    path: 'floating.corner',
    control: {
      kind: 'select',
      options: [
        { value: 'top_left', label: 'corner.top_left' },
        { value: 'top_right', label: 'corner.top_right' },
        { value: 'bottom_left', label: 'corner.bottom_left' },
        { value: 'bottom_right', label: 'corner.bottom_right' },
      ],
    },
    depends_on: 'floating.enabled',
  },
  {
    id: 'card_size',
    pane: 'floating',
    label: 'floating.card_size',
    help: 'floating.card_size.help',
    path: 'floating.card_size',
    control: {
      kind: 'select',
      options: [
        { value: 'default', label: 'size.default' },
        { value: 'compact', label: 'size.compact' },
      ],
    },
    depends_on: 'floating.enabled',
  },
  {
    id: 'auto_hide',
    pane: 'floating',
    label: 'floating.auto_hide',
    help: 'floating.auto_hide.help',
    path: 'floating.auto_hide_secs',
    control: { kind: 'number', min: 0, max: 3600, unit: 'unit.seconds' },
    depends_on: 'floating.enabled',
  },
  {
    id: 'follow_cursor',
    pane: 'floating',
    label: 'floating.follow_cursor',
    help: 'floating.follow_cursor.help',
    path: 'floating.follow_cursor',
    control: { kind: 'switch' },
    depends_on: 'floating.enabled',
  },
  {
    id: 'hover_hotkeys',
    pane: 'floating',
    label: 'floating.hover_hotkeys',
    help: 'floating.hover_hotkeys.help',
    path: 'floating.hover_hotkeys',
    control: { kind: 'switch' },
    depends_on: 'floating.enabled',
  },
  {
    id: 'svc_hk_optimise',
    pane: 'hotkeys',
    group: 'svc.group.global_hotkeys',
    label: 'tray.optimise',
    help: 'svc.hotkey.global.help',
    path: 'hotkeys.optimise',
    control: { kind: 'hotkey', action: 'optimise' },
  },
  {
    id: 'svc_hk_aggressive',
    pane: 'hotkeys',
    group: 'svc.group.global_hotkeys',
    label: 'tray.aggressive',
    help: 'svc.hotkey.global.help',
    path: 'hotkeys.aggressive',
    control: { kind: 'hotkey', action: 'aggressive' },
  },
  {
    id: 'svc_hk_cycle_state',
    pane: 'hotkeys',
    group: 'svc.group.global_hotkeys',
    label: 'svc.hotkey.cycle_state',
    help: 'svc.hotkey.global.help',
    path: 'hotkeys.cycle_state',
    control: { kind: 'hotkey', action: 'cycle_state' },
  },
  {
    id: 'svc_hk_preview',
    pane: 'hotkeys',
    group: 'svc.group.hover_hotkeys',
    label: 'svc.hotkey.preview',
    help: 'svc.hotkey.hover.help',
    path: 'svc.shortcuts.preview',
    control: { kind: 'hotkey', action: 'preview' },
    depends_on: 'floating.hover_hotkeys',
  },
  {
    id: 'svc_hk_compare',
    pane: 'hotkeys',
    group: 'svc.group.hover_hotkeys',
    label: 'svc.hotkey.compare',
    help: 'svc.hotkey.hover.help',
    path: 'svc.shortcuts.compare',
    control: { kind: 'hotkey', action: 'compare' },
    depends_on: 'floating.hover_hotkeys',
  },
  {
    id: 'svc_hk_step_down',
    pane: 'hotkeys',
    group: 'svc.group.hover_hotkeys',
    label: 'svc.hotkey.step_down',
    help: 'svc.hotkey.hover.help',
    path: 'svc.shortcuts.step_down',
    control: { kind: 'hotkey', action: 'step_down' },
    depends_on: 'floating.hover_hotkeys',
  },
  {
    id: 'svc_hk_scale',
    pane: 'hotkeys',
    group: 'svc.group.hover_hotkeys',
    label: 'svc.hotkey.scale',
    help: 'svc.hotkey.hover.help',
    control: { kind: 'hotkey', action: 'scale' },
    depends_on: 'floating.hover_hotkeys',
  },
  {
    id: 'svc_rail',
    pane: 'floating',
    label: 'svc.rail.label',
    help: 'svc.rail.help',
    path: 'svc.rail.tools',
    control: { kind: 'rail' },
    depends_on: 'floating.enabled',
    wide: true,
  },
  {
    id: 'svc_update_pill',
    pane: 'floating',
    label: 'svc.updates.pill',
    help: 'svc.updates.pill.help',
    path: 'svc.updates.pill',
    control: { kind: 'switch' },
    depends_on: 'floating.enabled',
  },
  {
    id: 'svc_update',
    pane: 'about',
    label: 'svc.updates.version',
    control: { kind: 'update' },
    wide: true,
  },
  {
    id: 'svc_auto_check',
    pane: 'about',
    label: 'svc.updates.auto',
    help: 'svc.updates.auto.help',
    path: 'svc.updates.auto_check',
    control: { kind: 'switch' },
  },
];

/** The settings to show on this OS. */
export function settingsFor(isMac: boolean): Setting[] {
  return all.filter((s) => isMac || !s.mac_only);
}
