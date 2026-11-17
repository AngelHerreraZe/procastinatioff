<script lang="ts">
  // Settings on this computer: the fixed goals, the technical report and "delete all my data". Relaxing a
  // goal and deleting are behind the local gate (a PIN, password or pattern set at onboarding or on the
  // phone); the service checks it and counts the failed attempts. There is no fingerprint or face on the
  // computer yet.
  import GoalsEditor from '../components/GoalsEditor.svelte';
  import SecretInput from '../components/SecretInput.svelte';
  import { authState, call, text, type AuthState, type Config, type Goal, type LocalFactor } from '../lib/agent';
  import { locale, t } from '../lib/i18n';

  let { onclose, onerror }: { onclose: () => void; onerror: (e: unknown) => void } = $props();

  let auth = $state<AuthState | null>(null);
  let factor = $state<LocalFactor | null>(null);
  let secret = $state(''); // pattern: "0-4-8-5"
  let config = $state<Config | null>(null);
  let goals = $state<Goal[]>([]);
  let saved = $state<string | null>(null);
  let report = $state<string | null>(null);
  let copied = $state(false);
  let confirming = $state(false);
  let busy = $state(false);

  const typed = (s: AuthState): LocalFactor[] => s.usable.filter((f) => f !== 'biometric');

  async function load() {
    auth = await authState();
    factor ??= auth ? (typed(auth)[0] ?? null) : null;
  }

  async function loadConfig() {
    const r = await call({ cmd: 'active_config' });
    if (r.type === 'config' && r.config) {
      config = r.config;
      goals = r.config.goals;
    }
  }

  $effect(() => {
    load().catch(onerror);
    loadConfig().catch(onerror);
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
      const value = secret;
      secret = '';
      await call({ cmd: 'authorize_local', factor, secret: value });
      await load();
    });

  // Tightening applies now; relaxing, at the next wake-up (the core decides and says when).
  const saveGoals = () =>
    run(async () => {
      saved = null;
      const r = await call({ cmd: 'set_config', config: $state.snapshot({ ...config, goals }) });
      if (r.type !== 'effective') return;
      const when = new Date(r.from_ms).toLocaleString(locale, { weekday: 'long', hour: '2-digit', minute: '2-digit' });
      saved = r.from_ms <= Date.now() ? t('settings.applies_now') : t('settings.applies_from', { when });
      await loadConfig();
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

  {#if auth && !auth.unlocked && typed(auth).length > 0}
    <section class="card stack">
      <h2>{t('settings.unlock')}</h2>
      <p>{t('settings.unlock_lead')}</p>
      {#if typed(auth).length > 1}
        <div class="row" role="group" aria-label={t('settings.method')}>
          {#each typed(auth) as f (f)}
            <button class="btn-tonal" aria-pressed={factor === f} onclick={() => ((factor = f), (secret = ''))}>
              {t(`settings.factor_${f}`)}
            </button>
          {/each}
        </div>
      {/if}
      {#if factor}<SecretInput {factor} bind:value={secret} />{/if}
      <button class="btn-primary" disabled={busy || (factor === 'pattern' ? secret.split('-').length < 4 : !secret)} onclick={unlock}>
        {t('settings.unlock')}
      </button>
    </section>
  {/if}

  {#if config}
    <section class="card stack" aria-labelledby="h-goals">
      <h2 id="h-goals">{t('onb.fixed_goals')}</h2>
      <GoalsEditor bind:goals />
      <p class="muted small">{t('onb.anti_impulse')}</p>
      <button class="btn-primary save" disabled={busy} onclick={saveGoals}>{t('settings.save_goals')}</button>
      {#if saved}<p role="status">{saved}</p>{/if}
    </section>
  {/if}

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
      <p>{t(typed(auth).length === 0 ? 'settings.no_typed_method' : 'settings.unlock_first')}</p>
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
  .back, .erase, .save { justify-self: start; }
  h2 { font-size: 18px; margin: 0; }
  .small { font-size: 14px; }
  .report { white-space: pre-wrap; font-size: 13px; background: var(--chrome); padding: 12px; border-radius: 8px; margin: 0; }
  [aria-pressed='true'].btn-tonal { outline: 2px solid var(--accent); }
</style>
