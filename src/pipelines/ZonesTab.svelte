<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t, type StringKey } from '../lib/i18n';
  import {
    moveZone,
    newZone,
    removeZone,
    ZONE_ICONS,
    type Media,
    type PipeMeta,
    type Pipelines,
    type Zone,
  } from '../lib/pipelines';
  import ZoneGrid from '../zones/ZoneGrid.svelte';

  let {
    pipelines,
    meta,
    onchange,
  }: { pipelines: Pipelines; meta: PipeMeta; onchange: (next: Pipelines) => void } = $props();

  const update = (id: number, patch: Partial<Zone>) =>
    onchange({ ...pipelines, zones: pipelines.zones.map((z) => (z.id === id ? { ...z, ...patch } : z)) });

  const assign = (zone: Zone, media: Media, value: string) =>
    update(zone.id, { pipelines: { ...zone.pipelines, [media]: value === '' ? null : Number(value) } });

  // The static preview shows each zone's pipeline name where the overlay shows its caption.
  const tiles = $derived(
    pipelines.zones.map((zone) => ({
      ...zone,
      captions: Object.fromEntries(
        meta.media.map((media) => [media, pipelines.library.find((p) => p.id === zone.pipelines[media])?.name ?? '']),
      ) as Partial<Record<Media, string>>,
    })),
  );
</script>

<div class="zones">
  <div class="list">
    {#each pipelines.zones as zone, i (zone.id)}
      <section class="zone" aria-label={zone.name}>
        <div class="row">
          <input
            type="text"
            aria-label={t('pipe.zones.name')}
            value={zone.name}
            onchange={(event) => update(zone.id, { name: event.currentTarget.value.trim() || t('pipe.zones.untitled') })}
          />
          <select
            aria-label={t('pipe.zones.icon')}
            value={zone.icon}
            onchange={(event) => update(zone.id, { icon: event.currentTarget.value })}
          >
            {#each ZONE_ICONS as icon (icon)}
              <option value={icon}>{t(`pipe.zones.icon.${icon}` as StringKey)}</option>
            {/each}
          </select>
          <button type="button" disabled={i === 0} aria-label={t('pipe.move_up')} onclick={() => onchange(moveZone(pipelines, i, i - 1))}>↑</button>
          <button
            type="button"
            disabled={i === pipelines.zones.length - 1}
            aria-label={t('pipe.move_down')}
            onclick={() => onchange(moveZone(pipelines, i, i + 1))}>↓</button
          >
          <button type="button" onclick={() => onchange(removeZone(pipelines, zone.id))}>{t('pipe.zones.remove')}</button>
        </div>
        {#each meta.media as media (media)}
          <label class="pick">
            <span>{t('pipe.zones.pipeline_for', { media: t(`pipe.media.${media}` as StringKey) })}</span>
            <select value={zone.pipelines[media] === null ? '' : String(zone.pipelines[media])} onchange={(event) => assign(zone, media, event.currentTarget.value)}>
              <option value="">{t('pipe.zones.none_choice')}</option>
              {#each pipelines.library.filter((p) => p.media === media) as p (p.id)}
                <option value={String(p.id)}>{p.name}</option>
              {/each}
            </select>
          </label>
        {/each}
      </section>
    {:else}
      <p class="empty">{t('pipe.zones.empty')}</p>
    {/each}
    <div class="actions">
      <button type="button" onclick={() => onchange(newZone(pipelines, t('pipe.zones.untitled')).lib)}>{t('pipe.zones.add')}</button>
      <button type="button" onclick={() => invoke('pipe_zones_show')}>{t('pipe.zones.preview')}</button>
    </div>
  </div>
  <div class="preview" aria-hidden="true">
    <ZoneGrid {tiles} />
  </div>
</div>

<style>
  .zones {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) auto;
    min-height: 0;
    overflow-y: auto;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
  }
  .zone {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: var(--bg-group);
    border: 1px solid var(--rail-line);
    border-radius: 8px;
  }
  .row,
  .pick {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .row input {
    flex: 1;
  }
  .pick span {
    flex: 1;
    color: var(--fg-2);
  }
  .pick select {
    width: 200px;
  }
  input,
  select,
  button {
    padding: 4px 8px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  button:not(:disabled) {
    cursor: pointer;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .preview {
    padding: 4px 16px;
    pointer-events: none;
  }
  .empty {
    color: var(--fg-2);
  }
  button:focus-visible,
  input:focus-visible,
  select:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
