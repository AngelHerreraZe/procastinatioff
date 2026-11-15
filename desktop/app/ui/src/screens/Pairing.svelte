<script lang="ts">
  // Vincular otro dispositivo: QR + código de 6 dígitos que se compara en ambas pantallas (anti-MITM).
  import { qrSvg } from '../lib/native';
  import { call, text } from '../lib/agent';

  let { onclose, onerror }: { onclose: () => void; onerror: (m: string) => void } = $props();

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
        if (alive) onerror(String(e));
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
      onerror(String(e));
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
  <button class="btn-text back" onclick={cancel}>Volver</button>
  {#if !code}
    <h1>Vincular otro dispositivo</h1>
    <p class="muted">Abre ProcastinaciOFF en el celular, elige «Unirme a un perfil» y escanea este código.</p>
    <div class="qr card" role="img" aria-label="Código QR de vinculación">
      {#if svg}{@html svg}{:else}<p class="muted">Generando código…</p>{/if}
    </div>
    <button
      class="btn-text copy"
      disabled={!offer}
      onclick={() => navigator.clipboard.writeText(offer).then(() => (copied = true), () => onerror('No se pudo copiar'))}
    >
      {copied ? 'Código copiado' : 'Copiar código (para otro PC)'}
    </button>
    <p class="muted small">El código caduca en 10 minutos. Tus metas viajan cifradas: el servidor no puede leerlas.</p>
  {:else}
    <h1>¿El otro dispositivo muestra este número?</h1>
    <p class="code num" aria-live="polite">{code}</p>
    <div class="row">
      <button class="btn-primary" disabled={busy} onclick={confirm}>Sí, coinciden</button>
      <button class="btn-secondary" onclick={cancel}>No coinciden: cancelar</button>
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
