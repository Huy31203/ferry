<script lang="ts">
  import { t } from '../lib/i18n';
  import type { Adjust } from '../lib/drop-ipc';

  let { adjust, onchange }: { adjust: Adjust; onchange: (next: Adjust) => void } = $props();

  const levels = [90, 80, 70, 60, 50, 40, 30, 20, 10];
  const formats = [
    ['png', 'PNG'],
    ['jpeg', 'JPEG'],
    ['webp', 'WebP'],
  ] as const;

  /** One select value for compression: a number, "extreme" or "" (from Settings). */
  const compression = $derived(adjust.extreme ? 'extreme' : String(adjust.compression ?? ''));

  function setCompression(value: string) {
    onchange({
      ...adjust,
      extreme: value === 'extreme',
      compression: value === '' || value === 'extreme' ? null : Number(value),
    });
  }
</script>

<aside>
  <p class="help">{t('drop.batch.adjust.help')}</p>
  <label>
    <span>{t('drop.batch.adjust.compression')}</span>
    <select value={compression} onchange={(e) => setCompression(e.currentTarget.value)}>
      <option value="">{t('drop.batch.adjust.from_settings')}</option>
      {#each levels as n (n)}
        <option value={String(n)}>{n}</option>
      {/each}
      <option value="extreme">{t('slider.extreme')}</option>
    </select>
  </label>
  <label>
    <span>{t('drop.batch.adjust.downscale')}</span>
    <select
      value={String(adjust.scale ?? '')}
      onchange={(e) => onchange({ ...adjust, scale: e.currentTarget.value === '' ? null : Number(e.currentTarget.value) })}
    >
      <option value="">{t('drop.batch.adjust.original_size')}</option>
      {#each levels as n (n)}
        <option value={String(n)}>{t('menu.percent', { n })}</option>
      {/each}
    </select>
  </label>
  <label>
    <span>{t('drop.batch.adjust.format')}</span>
    <select
      value={adjust.format ?? ''}
      onchange={(e) => onchange({ ...adjust, format: e.currentTarget.value === '' ? null : (e.currentTarget.value as Adjust['format']) })}
    >
      <option value="">{t('drop.batch.adjust.keep_format')}</option>
      {#each formats as [value, label] (value)}
        <option {value}>{label}</option>
      {/each}
    </select>
  </label>
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 200px;
    padding: 12px;
    border-left: 1px solid var(--rail-line);
    background: var(--bg-group);
  }
  .help {
    margin: 0;
    color: var(--fg-2);
    font-size: 12px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--fg-2);
  }
  select {
    font: inherit;
    color: var(--fg);
  }
</style>
