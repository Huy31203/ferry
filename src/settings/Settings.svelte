<script lang="ts">
  import { t } from '../lib/i18n';
</script>

<main>
  <h1>{t('settings.placeholder_title')}</h1>
  <p>{t('settings.placeholder_body')}</p>
</main>

<style>
  main {
    padding: 24px 28px;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 20px;
    font-weight: 600;
  }
  p {
    margin: 0;
    color: var(--fg-2);
  }
</style>
