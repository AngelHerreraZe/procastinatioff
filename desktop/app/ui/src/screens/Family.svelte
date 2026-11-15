<script lang="ts">
  // PC de un menor: pedir algo al tutor y "Lo que ve tu tutor" (panel espejo: exactamente lo mismo que ve él).
  import { call, familyView, type FamilyAlertView, type PendingRequest, type RequestKind, type Status } from '../lib/agent';
  import { hhmm } from '../lib/format';
  import Icon from '../components/Icon.svelte';

  let { status, onclose, onerror }: { status: Status; onclose: () => void; onerror: (m: string) => void } = $props();

  let requests = $state<PendingRequest[]>([]);
  let alerts = $state<FamilyAlertView[]>([]);
  let site = $state('');
  let busy = $state(false);
  let sent = $state('');

  const load = () =>
    familyView().then((v) => ({ requests, alerts } = v), (e) => onerror(String(e)));

  $effect(() => {
    load();
    const id = setInterval(load, 30_000);
    return () => clearInterval(id);
  });

  async function ask(what: RequestKind, label: string) {
    busy = true;
    try {
      await call({ cmd: 'family_request', what });
      sent = `Enviado: ${label}. Tu tutor recibe un aviso.`;
      site = '';
      await load();
    } catch (e) {
      onerror(String(e));
    } finally {
      busy = false;
    }
  }

  const describe = (w: RequestKind): string =>
    w.what === 'extra_permit' ? `${w.minutes} min más`
    : w.what === 'add_site' ? `Permitir ${w.domain} en el enfoque`
    : w.what === 'later_bedtime' ? `Dormir ${w.minutes} min más tarde`
    : 'Día de descanso';

  const TAMPER: Record<string, string> = {
    permission_revoked: 'Se desactivó un permiso de protección',
    service_stopped: 'La protección estuvo detenida',
    clock_changed: 'Se cambió la hora del dispositivo',
    uninstall_attempt: 'Se intentó desinstalar la app',
    safe_mode: 'Se arrancó en modo seguro',
    device_silent: 'Un dispositivo dejó de reportar',
  };
  const alertText = (a: FamilyAlertView): string =>
    a.alert.alert === 'tamper' ? TAMPER[a.alert.what]
    : a.alert.alert === 'emergency' ? 'Se usó la emergencia'
    : `${a.alert.count} ${a.alert.count === 1 ? 'sitio bloqueado' : 'sitios bloqueados'} por categoría (nunca cuáles)`;
  const when = (ms: number) =>
    `${new Date(ms).toLocaleDateString('es', { weekday: 'short', day: 'numeric', month: 'short' })}, ${hhmm(ms)}`;

  const done = $derived(status.goals.filter((g) => g.completed).length);
</script>

<main class="family stack">
  <button class="btn-text back" onclick={onclose}>Volver</button>
  <h1>Tu tutor</h1>

  <section class="card stack" aria-labelledby="ask-title">
    <h2 id="ask-title">Pedir algo</h2>
    <div class="row">
      <button class="btn-secondary" disabled={busy} onclick={() => ask({ what: 'extra_permit', minutes: 15 }, '15 min más')}>
        15 min más
      </button>
      <button class="btn-secondary" disabled={busy} onclick={() => ask({ what: 'rest_day' }, 'día de descanso')}>
        Día de descanso
      </button>
    </div>
    <form class="row" onsubmit={(e) => { e.preventDefault(); ask({ what: 'add_site', domain: site.trim() }, site.trim()); }}>
      <label class="grow">
        <span class="muted small">Un sitio para tus tareas</span>
        <input bind:value={site} placeholder="khanacademy.org" maxlength="253" autocomplete="off" />
      </label>
      <button class="btn-secondary" disabled={busy || !site.trim()}>Pedir acceso</button>
    </form>
    {#if sent}<p role="status">{sent}</p>{/if}
    {#if requests.length > 0}
      <h3>Esperando respuesta</h3>
      <ul>
        {#each requests as r (r.request_id)}
          <li>{describe(r.what)} <span class="muted">· {when(r.at_ms)}</span></li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="card stack" aria-labelledby="mirror-title">
    <h2 id="mirror-title"><Icon name="target" /> Lo que ve tu tutor</h2>
    <p class="muted">Tu tutor ve exactamente esto, ni más ni menos.</p>
    <p>Hoy: {done} de {status.goals.length} metas cumplidas · {status.permits_left} permisos disponibles.</p>
    {#if alerts.length === 0}
      <p class="muted">Sin avisos en los últimos 7 días.</p>
    {:else}
      <ul>
        {#each alerts as a (a.at_ms + a.device_label + a.alert.alert)}
          <li>{alertText(a)} <span class="muted">· {when(a.at_ms)} · {a.device_label}</span></li>
        {/each}
      </ul>
    {/if}
    <p class="muted small">Nunca ve tus mensajes, contactos, fotos, historial de navegación ni ubicación: no existen en el sistema.</p>
  </section>
</main>

<style>
  .family { max-width: 720px; margin: 0 auto; padding: 24px 16px 48px; }
  .back { justify-self: start; }
  .card { padding: 24px; }
  .grow { flex: 1; display: grid; gap: 4px; min-width: 200px; }
  .small { font-size: 14px; }
  ul { margin: 0; padding-left: 20px; display: grid; gap: 6px; }
  h2 { display: flex; align-items: center; gap: 8px; }
</style>
