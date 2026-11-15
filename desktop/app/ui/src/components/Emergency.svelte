<script lang="ts">
  // "Break the glass": protective friction (wait 60 s + write the reason). The reason is NOT saved or sent.
  import { call } from '../lib/agent';
  import { t, tn } from '../lib/i18n';
  import Icon from './Icon.svelte';

  let { minutes = 15, penalty = 1, onerror }: { minutes?: number; penalty?: number; onerror: (e: unknown) => void } =
    $props();

  const WAIT = 60;
  let open = $state(false);
  let reason = $state('');
  let left = $state(WAIT);

  $effect(() => {
    if (!open) return;
    left = WAIT;
    const id = setInterval(() => (left = Math.max(0, left - 1)), 1000);
    return () => clearInterval(id);
  });

  async function use() {
    try {
      await call({ cmd: 'emergency' });
      open = false;
      reason = '';
    } catch (e) {
      onerror(e);
    }
  }
</script>

{#if !open}
  <button class="btn-danger" onclick={() => (open = true)}>
    <span class="row" style="gap:6px"><Icon name="alert" size={18} /> {t('emergency.title')}</span>
  </button>
{:else}
  <section class="card emergency stack" aria-labelledby="emergency-title">
    <h3 id="emergency-title">{t('emergency.title')}</h3>
    <p class="muted">
      {t('emergency.gives', { minutes })}
      {penalty > 0 ? tn('emergency.cost', penalty) : t('emergency.recorded')}
    </p>
    <label>
      <span>{t('emergency.why')}</span>
      <textarea rows="2" bind:value={reason} maxlength="200"></textarea>
      <small>{t('emergency.not_saved')}</small>
    </label>
    <div class="row">
      <button class="btn-secondary" disabled={left > 0 || reason.trim().length < 10} onclick={use}>
        {left > 0 ? t('common.wait_seconds', { n: left }) : t('emergency.use', { minutes })}
      </button>
      <button class="btn-text" onclick={() => (open = false)}>{t('common.cancel')}</button>
    </div>
    <p class="sr-only" aria-live="polite">{left === 0 ? t('emergency.ready') : ''}</p>
  </section>
{/if}

<style>
  .emergency { border-color: var(--danger); max-width: 520px; }
  .emergency h3 { color: var(--danger); }
</style>
