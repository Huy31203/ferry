import type { StringKey } from './i18n';
import type { HotkeyId } from './svc';

export type PaneId =
  | 'general'
  | 'clipboard'
  | 'images'
  | 'videos' // stream A: video
  | 'audio' // stream B: audio
  | 'files'
  | 'floating'
  | 'hotkeys' // svc
  | 'about'; // svc

export const panes: { id: PaneId; label: StringKey }[] = [
  { id: 'general', label: 'pane.general' },
  { id: 'clipboard', label: 'pane.clipboard' },
  { id: 'images', label: 'pane.images' },
  { id: 'videos', label: 'video.pane' }, // stream A: video
  { id: 'audio', label: 'audio.pane' }, // stream B: audio
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
  | { kind: 'output'; converted: boolean; media?: 'video' | 'audio' }
  | { kind: 'encoder' } // stream A: video
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
  },
  {
    id: 'copy_paths',
    pane: 'clipboard',
    label: 'clipboard.copy_paths',
    help: 'clipboard.copy_paths.help',
    path: 'clipboard.copy_paths',
    control: { kind: 'switch' },
  },
  // sI clipboard-fixes
  {
    id: 'auto_copy',
    pane: 'clipboard',
    label: 'clip.auto_copy',
    help: 'clip.auto_copy.help',
    path: 'clip.auto_copy',
    control: { kind: 'switch' },
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

  // stream A: video
  {
    id: 'video_encoder',
    pane: 'videos',
    label: 'video.encoder',
    help: 'video.encoder.help',
    path: 'video.encoder',
    control: { kind: 'encoder' },
  },
  {
    id: 'video_compression',
    pane: 'videos',
    label: 'video.compression',
    help: 'video.compression.help',
    path: 'video.compression',
    control: { kind: 'slider', min: 5, max: 100 },
  },
  {
    id: 'video_convert_mov',
    pane: 'videos',
    label: 'video.convert_mov',
    help: 'video.convert_mov.help',
    path: 'video.convert_mov',
    control: { kind: 'switch' },
  },
  {
    id: 'video_audio_to_aac',
    pane: 'videos',
    label: 'video.audio_to_aac',
    help: 'video.audio_to_aac.help',
    path: 'video.audio_to_aac',
    control: { kind: 'switch' },
  },
  {
    id: 'video_remove_audio',
    pane: 'videos',
    label: 'video.remove_audio',
    help: 'video.remove_audio.help',
    path: 'video.remove_audio',
    control: { kind: 'switch' },
  },
  {
    id: 'video_fps_cap',
    pane: 'videos',
    label: 'video.fps_cap',
    help: 'video.fps_cap.help',
    path: 'video.fps_cap',
    control: {
      kind: 'select',
      options: [
        { value: 'off', label: 'video.fps_cap.off' },
        { value: '24', label: 'video.fps_cap.24' },
        { value: '30', label: 'video.fps_cap.30' },
        { value: '60', label: 'video.fps_cap.60' },
      ],
    },
  },
  {
    id: 'video_speed_frames',
    pane: 'videos',
    label: 'video.speed_frames',
    help: 'video.speed_frames.help',
    path: 'video.speed_frames',
    control: {
      kind: 'select',
      options: [
        { value: 'keep', label: 'video.speed.keep_frames' },
        { value: 'drop', label: 'video.speed.drop_frames' },
      ],
    },
  },
  {
    id: 'video_preserve_dates',
    pane: 'videos',
    label: 'video.preserve_dates',
    help: 'video.preserve_dates.help',
    path: 'video.preserve_dates',
    control: { kind: 'switch' },
  },
  {
    id: 'video_ignored_extensions',
    pane: 'videos',
    label: 'video.ignored_extensions',
    help: 'video.ignored_extensions.help',
    path: 'video.ignored_extensions',
    control: { kind: 'extensions' },
  },
  {
    id: 'video_max_size',
    pane: 'videos',
    label: 'video.max_size',
    help: 'video.max_size.help',
    path: 'video.max_size_mb',
    control: { kind: 'number', min: 1, max: 100000, unit: SIZE_UNIT },
  },

  // ---- stream B: audio ----
  {
    id: 'audio_out_wav',
    pane: 'audio',
    group: 'audio.group.formats',
    label: 'audio.outputs.wav',
    help: 'audio.outputs.help',
    path: 'audio.outputs.wav',
    control: { kind: 'select', options: [{ value: 'mp3', label: 'audio.format.mp3' }, { value: 'm4a', label: 'audio.format.m4a' }, { value: 'opus', label: 'audio.format.opus' }, { value: 'wav', label: 'audio.format.wav' }, { value: 'flac', label: 'audio.format.flac' }] },
  },
  {
    id: 'audio_out_aiff',
    pane: 'audio',
    group: 'audio.group.formats',
    label: 'audio.outputs.aiff',
    path: 'audio.outputs.aiff',
    control: { kind: 'select', options: [{ value: 'mp3', label: 'audio.format.mp3' }, { value: 'm4a', label: 'audio.format.m4a' }, { value: 'opus', label: 'audio.format.opus' }, { value: 'wav', label: 'audio.format.wav' }, { value: 'flac', label: 'audio.format.flac' }] },
  },
  {
    id: 'audio_out_flac',
    pane: 'audio',
    group: 'audio.group.formats',
    label: 'audio.outputs.flac',
    path: 'audio.outputs.flac',
    control: { kind: 'select', options: [{ value: 'mp3', label: 'audio.format.mp3' }, { value: 'm4a', label: 'audio.format.m4a' }, { value: 'opus', label: 'audio.format.opus' }, { value: 'wav', label: 'audio.format.wav' }, { value: 'flac', label: 'audio.format.flac' }] },
  },
  {
    id: 'audio_out_mp3',
    pane: 'audio',
    group: 'audio.group.formats',
    label: 'audio.outputs.mp3',
    path: 'audio.outputs.mp3',
    control: { kind: 'select', options: [{ value: 'mp3', label: 'audio.format.mp3' }, { value: 'm4a', label: 'audio.format.m4a' }, { value: 'opus', label: 'audio.format.opus' }, { value: 'wav', label: 'audio.format.wav' }, { value: 'flac', label: 'audio.format.flac' }] },
  },
  {
    id: 'audio_out_m4a',
    pane: 'audio',
    group: 'audio.group.formats',
    label: 'audio.outputs.m4a',
    path: 'audio.outputs.m4a',
    control: { kind: 'select', options: [{ value: 'mp3', label: 'audio.format.mp3' }, { value: 'm4a', label: 'audio.format.m4a' }, { value: 'opus', label: 'audio.format.opus' }, { value: 'wav', label: 'audio.format.wav' }, { value: 'flac', label: 'audio.format.flac' }] },
  },
  {
    id: 'audio_out_ogg',
    pane: 'audio',
    group: 'audio.group.formats',
    label: 'audio.outputs.ogg',
    path: 'audio.outputs.ogg',
    control: { kind: 'select', options: [{ value: 'mp3', label: 'audio.format.mp3' }, { value: 'm4a', label: 'audio.format.m4a' }, { value: 'opus', label: 'audio.format.opus' }, { value: 'wav', label: 'audio.format.wav' }, { value: 'flac', label: 'audio.format.flac' }] },
  },
  {
    id: 'audio_out_opus',
    pane: 'audio',
    group: 'audio.group.formats',
    label: 'audio.outputs.opus',
    path: 'audio.outputs.opus',
    control: { kind: 'select', options: [{ value: 'mp3', label: 'audio.format.mp3' }, { value: 'm4a', label: 'audio.format.m4a' }, { value: 'opus', label: 'audio.format.opus' }, { value: 'wav', label: 'audio.format.wav' }, { value: 'flac', label: 'audio.format.flac' }] },
  },
  {
    id: 'audio_bitrate',
    pane: 'audio',
    label: 'audio.bitrate',
    help: 'audio.bitrate.help',
    path: 'audio.bitrate_kbps',
    control: { kind: 'number', min: 32, max: 320, unit: 'audio.unit.kbps' },
  },
  {
    id: 'audio_opus_bitrate',
    pane: 'audio',
    label: 'audio.opus_bitrate',
    help: 'audio.opus_bitrate.help',
    path: 'audio.opus_bitrate_kbps',
    control: { kind: 'number', min: 16, max: 256, unit: 'audio.unit.kbps' },
  },
  {
    id: 'audio_loudness',
    pane: 'audio',
    label: 'audio.loudness',
    help: 'audio.loudness.help',
    path: 'audio.loudness',
    control: { kind: 'select', options: [{ value: 'off', label: 'audio.loudness.off' }, { value: 'streaming', label: 'audio.loudness.streaming' }, { value: 'apple_music', label: 'audio.loudness.apple_music' }, { value: 'podcast', label: 'audio.loudness.podcast' }, { value: 'broadcast', label: 'audio.loudness.broadcast' }] },
  },
  {
    id: 'audio_cover',
    pane: 'audio',
    label: 'audio.cover',
    help: 'audio.cover.help',
    path: 'audio.cover',
    control: { kind: 'select', options: [{ value: 'keep', label: 'audio.cover.keep' }, { value: 'optimise', label: 'audio.cover.optimise' }, { value: 'downscale', label: 'audio.cover.downscale' }, { value: 'remove', label: 'audio.cover.remove' }] },
  },
  {
    id: 'audio_cover_shape',
    pane: 'audio',
    label: 'audio.cover_shape',
    help: 'audio.cover_shape.help',
    path: 'audio.cover_size.shape',
    control: { kind: 'select', options: [{ value: 'square', label: 'audio.cover_shape.square' }, { value: 'long_edge', label: 'audio.cover_shape.long_edge' }] },
  },
  {
    id: 'audio_cover_side',
    pane: 'audio',
    label: 'audio.cover_side',
    help: 'audio.cover_side.help',
    path: 'audio.cover_size.side',
    control: { kind: 'number', min: 100, max: 3000, unit: 'audio.unit.px' },
  },
  {
    id: 'audio_ignored_extensions',
    pane: 'audio',
    label: 'audio.ignored_extensions',
    help: 'audio.ignored_extensions.help',
    path: 'audio.ignored_extensions',
    control: { kind: 'extensions' },
  },
  {
    id: 'audio_max_size',
    pane: 'audio',
    label: 'audio.max_size',
    help: 'audio.max_size.help',
    path: 'audio.max_size_mb',
    control: { kind: 'number', min: 1, max: 100000, unit: SIZE_UNIT },
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
    id: 'video_output_optimised',
    pane: 'files',
    group: 'video.group',
    label: 'output.optimised',
    help: 'video.output.optimised.help',
    path: 'video.output.optimised',
    control: { kind: 'output', converted: false, media: 'video' },
  },
  {
    id: 'video_output_converted',
    pane: 'files',
    group: 'video.group',
    label: 'output.converted',
    help: 'video.output.converted.help',
    path: 'video.output.converted',
    control: { kind: 'output', converted: true, media: 'video' },
  },
  {
    id: 'audio_output_optimised',
    pane: 'files',
    group: 'audio.group.files',
    label: 'output.optimised',
    help: 'output.optimised.help',
    path: 'audio.output.optimised',
    control: { kind: 'output', converted: false, media: 'audio' },
  },
  {
    id: 'audio_output_converted',
    pane: 'files',
    group: 'audio.group.files',
    label: 'output.converted',
    help: 'output.converted.help',
    path: 'audio.output.converted',
    control: { kind: 'output', converted: true, media: 'audio' },
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
