import type { Setting } from './schema';

/** The Sharing pane: an editor per kind of file, the uploader and the shelf apps. */
export const shareSettings: Setting[] = [
  {
    id: 'edit_with_image',
    pane: 'sharing',
    group: 'share.group.edit_with',
    label: 'share.edit_with.image',
    help: 'share.edit_with.image.help',
    path: 'share.edit_with.image',
    control: { kind: 'app' },
  },
  {
    id: 'edit_with_video',
    pane: 'sharing',
    label: 'share.edit_with.video',
    path: 'share.edit_with.video',
    control: { kind: 'app' },
  },
  {
    id: 'edit_with_audio',
    pane: 'sharing',
    label: 'share.edit_with.audio',
    path: 'share.edit_with.audio',
    control: { kind: 'app' },
  },
  {
    id: 'edit_with_pdf',
    pane: 'sharing',
    label: 'share.edit_with.pdf',
    path: 'share.edit_with.pdf',
    control: { kind: 'app' },
  },
  {
    id: 'uploader',
    pane: 'sharing',
    group: 'share.group.upload',
    label: 'share.uploader',
    help: 'share.uploader.help',
    path: 'share.uploader',
    control: { kind: 'app' },
  },
  {
    id: 'shelf_apps',
    pane: 'sharing',
    group: 'share.group.upload',
    label: 'share.shelf_apps',
    help: 'share.shelf_apps.help',
    path: 'share.shelf_apps',
    control: { kind: 'app_list' },
  },
];
