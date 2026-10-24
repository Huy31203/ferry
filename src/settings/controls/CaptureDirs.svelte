<script lang="ts">
  import { statusText, statusTone } from '../../lib/watch';
  import { watch } from '../watchStore.svelte';

  let { labelledby }: { labelledby: string } = $props();
</script>

<ul aria-labelledby={labelledby}>
  {#each watch.status?.captures ?? [] as dir (dir.path)}
    <li>
      <span class="path" title={dir.path}>{dir.path}</span>
      <span class="chip {statusTone(dir.state)}">{statusText(dir.state, true)}</span>
    </li>
  {/each}
</ul>

<style>
  ul {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .path {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip {
    flex-shrink: 0;
    padding: 2px 8px;
    font-size: 11px;
    border: 1px solid currentColor;
    border-radius: 999px;
  }
  .ok {
    color: #137a3d;
  }
  .warn {
    color: #8a5a00;
  }
  .off {
    color: var(--fg-2);
  }
  .error {
    color: var(--error);
  }
  @media (prefers-color-scheme: dark) {
    .ok {
      color: #6fd39a;
    }
    .warn {
      color: #f0c060;
    }
  }
</style>
