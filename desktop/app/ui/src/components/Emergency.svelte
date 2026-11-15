<script lang="ts">
  // "Romper el cristal": fricción que protege (esperar 60 s + escribir el motivo). El motivo NO se guarda ni se envía.
  import { call } from '../lib/agent';
  import Icon from './Icon.svelte';

  let { minutes = 15, penalty = 1, onerror }: { minutes?: number; penalty?: number; onerror: (m: string) => void } =
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
      onerror(String(e));
    }
  }
</script>

{#if !open}
  <button class="btn-danger" onclick={() => (open = true)}>
    <span class="row" style="gap:6px"><Icon name="alert" size={18} /> Emergencia</span>
  </button>
{:else}
  <section class="card emergency stack" aria-labelledby="emergency-title">
    <h3 id="emergency-title">Emergencia</h3>
    <p class="muted">
      Te da {minutes} minutos libres. {#if penalty > 0}Cuesta {penalty} {penalty === 1 ? 'permiso' : 'permisos'} mañana y
        queda registrado.{:else}Queda registrado.{/if}
    </p>
    <label>
      <span>¿Por qué lo necesitas?</span>
      <textarea rows="2" bind:value={reason} maxlength="200"></textarea>
      <small>El motivo no se guarda ni se envía a nadie.</small>
    </label>
    <div class="row">
      <button class="btn-secondary" disabled={left > 0 || reason.trim().length < 10} onclick={use}>
        {left > 0 ? `Espera ${left} s` : `Usar emergencia (${minutes} min)`}
      </button>
      <button class="btn-text" onclick={() => (open = false)}>Cancelar</button>
    </div>
    <p class="sr-only" aria-live="polite">{left === 0 ? 'Ya puedes usar la emergencia' : ''}</p>
  </section>
{/if}

<style>
  .emergency { border-color: var(--danger); max-width: 520px; }
  .emergency h3 { color: var(--danger); }
</style>
