<script lang="ts">
  // Settings on this computer: the technical report and "delete all my data". Deleting is behind the
  // local gate (a PIN, password or pattern set on the phone); the service checks it and counts the
  // failed attempts. There is no fingerprint or face on the computer yet.
  import { authState, call, text, type AuthState, type LocalFactor } from '../lib/agent';
  import { t } from '../lib/i18n';

  let { onclose, onerror }: { onclose: () => void; onerror: (e: unknown) => void } = $props();

  let auth = $state<AuthState | null>(null);
  let factor = $state<LocalFactor | null>(null);
  let secret = $state('');
  let cells = $state<number[]>([]); // pattern: cells of a 3×3 grid, in order
  let report = $state<string | null>(null);
  let copied = $state(false);
  let confirming = $state(false);
  let busy = $state(false);

  const typed = (s: AuthState): LocalFactor[] => s.usable.filter((f) => f !== 'biometric');

  async function load() {
    auth = await authState();
    factor ??= auth ? (typed(auth)[0] ?? null) : null;
  }

  $effect(() => {
    load().catch(onerror);
    // Leaving the screen closes the gate again
    return () => {
      call({ cmd: 'lock_settings' }).catch(() => {});
    };
  });

  async function run(action: () => Promise<void>) {
    busy = true;
    try {
      await action();
    } catch (e) {
      onerror(e);
    } finally {
      busy = false;
    }
  }

  const unlock = () =>
    run(async () => {
      const value = factor === 'pattern' ? cells.join('-') : secret;
      secret = '';
      cells = [];
      await call({ cmd: 'authorize_local', factor, secret: value });
      await load();
    });

  const createReport = () =>
    run(async () => {
      report = await text({ cmd: 'diagnostic_report', platform: [`ui=desktop platform=${navigator.platform}`] });
    });

  // Without a profile the app shows the welcome screen by itself.
  const erase = () =>
    run(async () => {
      await call({ cmd: 'erase_all' });
      onclose();
    });
</script>

<main class="settings stack">
  <button class="btn-text back" onclick={onclose}>{t('common.back')}</button>
  <h1>{t('settings.title')}</h1>

  <section class="card stack">
    <h2>{t('settings.report_title')}</h2>
    <p class="muted">{t('settings.report_lead')}</p>
    {#if report}
      <pre class="report">{report}</pre>
      <button
        class="btn-secondary"
        onclick={() => navigator.clipboard.writeText(report ?? '').then(() => (copied = true), () => onerror('error.copy'))}
      >
        {copied ? t('pair.copied') : t('settings.report_copy')}
      </button>
    {:else}
      <button class="btn-secondary" disabled={busy} onclick={createReport}>{t('settings.report_create')}</button>
    {/if}
  </section>

  <section class="card stack">
    <h2>{t('settings.erase_title')}</h2>
    <p class="muted">{t('settings.erase_lead')}</p>
    {#if !auth}
      <p class="muted">{t('app.connecting')}</p>
    {:else if !auth.unlocked}
      {#if typed(auth).length === 0}
        <p>{t('settings.no_typed_method')}</p>
      {:else}
        <p>{t('settings.unlock_lead')}</p>
        {#if typed(auth).length > 1}
          <div class="row" role="group" aria-label={t('settings.method')}>
            {#each typed(auth) as f (f)}
              <button class="btn-tonal" aria-pressed={factor === f} onclick={() => ((factor = f), (secret = ''), (cells = []))}>
                {t(`settings.factor_${f}`)}
              </button>
            {/each}
          </div>
        {/if}
        {#if factor === 'pattern'}
          <div class="grid" role="group" aria-label={t('settings.factor_pattern')}>
            {#each [0, 1, 2, 3, 4, 5, 6, 7, 8] as i (i)}
              <button
                class="cell"
                aria-pressed={cells.includes(i)}
                onclick={() => {
                  if (!cells.includes(i)) cells = [...cells, i];
                }}>{cells.includes(i) ? cells.indexOf(i) + 1 : ''}</button
              >
            {/each}
          </div>
          <button class="btn-text" onclick={() => (cells = [])}>{t('settings.pattern_clear')}</button>
        {:else}
          <label>
            <span>{t(`settings.factor_${factor}`)}</span>
            <input type="password" inputmode={factor === 'pin' ? 'numeric' : 'text'} autocomplete="off" bind:value={secret} />
          </label>
        {/if}
        <button class="btn-primary" disabled={busy || (factor === 'pattern' ? cells.length < 4 : !secret)} onclick={unlock}>
          {t('settings.unlock')}
        </button>
      {/if}
    {:else if auth.contact_required}
      <p>{t('settings.erase_needs_contact')}</p>
    {:else if !confirming}
      <button class="btn-danger erase" onclick={() => (confirming = true)}>{t('settings.erase_button')}</button>
    {:else}
      <p class="error" role="alert">{t('settings.erase_confirm')}</p>
      <div class="row">
        <button class="btn-primary" disabled={busy} onclick={erase}>{t('settings.erase_confirm_button')}</button>
        <button class="btn-secondary" onclick={() => (confirming = false)}>{t('common.cancel')}</button>
      </div>
    {/if}
  </section>
</main>

<style>
  .settings { max-width: 640px; margin: 0 auto; padding: 24px 16px 48px; }
  .back, .erase { justify-self: start; }
  h2 { font-size: 18px; margin: 0; }
  .report { white-space: pre-wrap; font-size: 13px; background: var(--chrome); padding: 12px; border-radius: 8px; margin: 0; }
  .grid { display: grid; grid-template-columns: repeat(3, 56px); gap: 12px; }
  .cell { width: 56px; height: 56px; border-radius: 50%; border: 1.5px solid var(--border); background: var(--surface); font-weight: 700; }
  .cell[aria-pressed='true'] { background: var(--accent-soft); border-color: var(--accent); color: var(--accent); }
  [aria-pressed='true'].btn-tonal { outline: 2px solid var(--accent); }
</style>
