<script lang="ts">
  // A minor's PC: ask the guardian for something and "What your guardian sees" (mirror panel: exactly what they see).
  import { call, familyView, type FamilyAlertView, type PendingRequest, type RequestKind, type Status } from '../lib/agent';
  import { hhmm } from '../lib/format';
  import { locale, t, tn } from '../lib/i18n';
  import Icon from '../components/Icon.svelte';

  let { status, onclose, onerror }: { status: Status; onclose: () => void; onerror: (e: unknown) => void } = $props();

  let requests = $state<PendingRequest[]>([]);
  let alerts = $state<FamilyAlertView[]>([]);
  let site = $state('');
  let busy = $state(false);
  let sent = $state('');

  const load = () =>
    familyView().then((v) => ({ requests, alerts } = v), (e) => onerror(e));

  $effect(() => {
    load();
    const id = setInterval(load, 30_000);
    return () => clearInterval(id);
  });

  async function ask(what: RequestKind) {
    busy = true;
    try {
      await call({ cmd: 'family_request', what });
      sent = t('family.sent', { what: describe(what) });
      site = '';
      await load();
    } catch (e) {
      onerror(e);
    } finally {
      busy = false;
    }
  }

  const describe = (w: RequestKind): string =>
    w.what === 'extra_permit' ? t('family.req_extra', { n: w.minutes })
    : w.what === 'add_site' ? t('family.req_site', { domain: w.domain })
    : w.what === 'later_bedtime' ? t('family.req_bedtime', { n: w.minutes })
    : t('family.rest_day');

  const alertText = (a: FamilyAlertView): string =>
    a.alert.alert === 'tamper' ? t(`tamper.${a.alert.what}`)
    : a.alert.alert === 'emergency' ? t('family.emergency_used')
    : tn('family.blocked_sites', a.alert.count);
  const when = (ms: number) =>
    `${new Date(ms).toLocaleDateString(locale, { weekday: 'short', day: 'numeric', month: 'short' })}, ${hhmm(ms)}`;

  const done = $derived(status.goals.filter((g) => g.completed).length);
</script>

<main class="family stack">
  <button class="btn-text back" onclick={onclose}>{t('common.back')}</button>
  <h1>{t('home.your_guardian')}</h1>

  <section class="card stack" aria-labelledby="ask-title">
    <h2 id="ask-title">{t('family.ask_title')}</h2>
    <div class="row">
      <button class="btn-secondary" disabled={busy} onclick={() => ask({ what: 'extra_permit', minutes: 15 })}>
        {t('family.more_15')}
      </button>
      <button class="btn-secondary" disabled={busy} onclick={() => ask({ what: 'rest_day' })}>
        {t('family.rest_day')}
      </button>
    </div>
    <form class="row" onsubmit={(e) => { e.preventDefault(); ask({ what: 'add_site', domain: site.trim() }); }}>
      <label class="grow">
        <span class="muted small">{t('family.site_label')}</span>
        <input bind:value={site} placeholder="khanacademy.org" maxlength="253" autocomplete="off" />
      </label>
      <button class="btn-secondary" disabled={busy || !site.trim()}>{t('family.ask_access')}</button>
    </form>
    {#if sent}<p role="status">{sent}</p>{/if}
    {#if requests.length > 0}
      <h3>{t('family.waiting')}</h3>
      <ul>
        {#each requests as r (r.request_id)}
          <li>{describe(r.what)} <span class="muted">· {when(r.at_ms)}</span></li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="card stack" aria-labelledby="mirror-title">
    <h2 id="mirror-title"><Icon name="target" /> {t('family.mirror_title')}</h2>
    <p class="muted">{t('family.mirror_note')}</p>
    <p>{t('family.today', { done, total: status.goals.length, passes: tn('home.passes_available', status.permits_left) })}</p>
    {#if alerts.length === 0}
      <p class="muted">{t('family.no_alerts')}</p>
    {:else}
      <ul>
        {#each alerts as a (a.at_ms + a.device_label + a.alert.alert)}
          <li>{alertText(a)} <span class="muted">· {when(a.at_ms)} · {a.device_label}</span></li>
        {/each}
      </ul>
    {/if}
    <p class="muted small">{t('family.never_sees')}</p>
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
