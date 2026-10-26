<script lang="ts">
  import { t, type StringKey } from '../lib/i18n';
  import { assign, usage, type Pipeline, type Pipelines, type Slot } from '../lib/pipelines';
  import type { WatchedFolder } from '../lib/settings';
  import Switch from '../settings/controls/Switch.svelte';

  let {
    pipelines,
    folders,
    pipeline,
    onchange,
  }: {
    pipelines: Pipelines;
    folders: WatchedFolder[];
    pipeline: Pipeline;
    onchange: (next: Pipelines) => void;
  } = $props();

  const media = $derived(t(`pipe.media.${pipeline.media}` as StringKey).toLowerCase());

  const rows = $derived<{ slot: Slot; label: string; on: boolean }[]>([
    {
      slot: { kind: 'clipboard' },
      label: t('pipe.assign.clipboard', { media }),
      on: pipelines.clipboard[pipeline.media] === pipeline.id,
    },
    {
      slot: { kind: 'drop_zone' },
      label: t('pipe.assign.drop_zone'),
      on: pipelines.drop_zone[pipeline.media] === pipeline.id,
    },
    ...pipelines.zones.map((zone) => ({
      slot: { kind: 'zone', zone: zone.id } as Slot,
      label: t('pipe.assign.zone', { name: zone.name }),
      on: zone.pipelines[pipeline.media] === pipeline.id,
    })),
  ]);

  const unassigned = $derived(usage(pipelines, pipeline.id, folders).length === 0);
</script>

<section>
  <h3>{t('pipe.assign.title')}</h3>
  {#each rows as row, n (n)}
    <div class="row">
      <span id={`assign-${n}`}>{row.label}</span>
      <Switch
        checked={row.on}
        labelledby={`assign-${n}`}
        onchange={(on) => onchange(assign(pipelines, row.slot, pipeline.media, on ? pipeline.id : null))}
      />
    </div>
  {/each}
  <p class="note">{t('pipe.assign.folders')}</p>
  {#if unassigned}
    <p class="warn" role="status">{t('pipe.assign.none')}</p>
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h3 {
    margin: 0;
    font-size: 13px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .note {
    margin: 0;
    font-size: 11px;
    color: var(--fg-2);
  }
  .warn {
    margin: 0;
    padding: 4px 8px;
    font-size: 11px;
    color: #8a5a00;
    background: color-mix(in srgb, #f0c060 25%, transparent);
    border-radius: 6px;
  }
  @media (prefers-color-scheme: dark) {
    .warn {
      color: #f0c060;
    }
  }
</style>
