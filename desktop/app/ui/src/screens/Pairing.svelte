<script lang="ts">
  // Pair another device: QR + 6-digit code compared on both screens (anti-MITM).
  import { qrSvg } from '../lib/native';
  import { call, text } from '../lib/agent';
  import { t } from '../lib/i18n';

  let { onclose, onerror }: { onclose: () => void; onerror: (e: unknown) => void } = $props();

  let svg = $state('');
  let offer = $state('');
  let copied = $state(false);
  let code = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    let alive = true;
    (async () => {
      try {
        offer = (await text({ cmd: 'pairing_offer' })) ?? '';
        if (!offer) return;
        svg = await qrSvg(offer);
        while (alive && !code) {
          await new Promise((r) => setTimeout(r, 2000));
          code = await text({ cmd: 'pairing_poll' });
        }
      } catch (e) {
        if (alive) onerror(e);
      }
    })();
    return () => {
      alive = false;
    };
  });

  async function confirm() {
    busy = true;
    try {
      await call({ cmd: 'pairing_confirm' });
      onclose();
    } catch (e) {
      onerror(e);
    } finally {
      busy = false;
    }
  }

  async function cancel() {
    await call({ cmd: 'pairing_cancel' }).catch(() => {});
    onclose();
  }
</script>

<main class="pair stack">
  <button class="btn-text back" onclick={cancel}>{t('common.back')}</button>
  {#if !code}
    <h1>{t('pair.title')}</h1>
    <p class="muted">{t('pair.instructions')}</p>
    <div class="qr card" role="img" aria-label={t('pair.qr_label')}>
      {#if svg}{@html svg}{:else}<p class="muted">{t('pair.generating')}</p>{/if}
    </div>
    <button
      class="btn-text copy"
      disabled={!offer}
      onclick={() => navigator.clipboard.writeText(offer).then(() => (copied = true), () => onerror('error.copy'))}
    >
      {copied ? t('pair.copied') : t('pair.copy')}
    </button>
    <p class="muted small">{t('pair.expires')}</p>
  {:else}
    <h1>{t('pair.match_question')}</h1>
    <p class="code num" aria-live="polite">{code}</p>
    <div class="row">
      <button class="btn-primary" disabled={busy} onclick={confirm}>{t('pair.match_yes')}</button>
      <button class="btn-secondary" onclick={cancel}>{t('pair.match_no')}</button>
    </div>
  {/if}
</main>

<style>
  .pair { max-width: 640px; margin: 0 auto; padding: 24px 16px 48px; }
  .back, .copy { justify-self: start; }
  .qr { display: grid; place-items: center; width: 304px; height: 304px; padding: 16px; background: #ffffff; }
  .qr :global(svg) { width: 272px; height: 272px; }
  .code { font-size: 56px; letter-spacing: 0.12em; font-weight: 700; }
  .small { font-size: 14px; }
</style>
