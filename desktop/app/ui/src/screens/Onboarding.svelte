<script lang="ts">
  // Onboarding: create the profile on this computer or join an existing one with another device's code.
  import { ALL_DAYS, call, methodsFor, slug, text, type Config, type Goal } from '../lib/agent';
  import { duration, toMinutes } from '../lib/format';
  import { t, tn, weekdayNames } from '../lib/i18n';

  let { onerror, ondone }: { onerror: (e: unknown) => void; ondone: () => void } = $props();

  const DAY_LABELS = weekdayNames('narrow');
  const DAY_NAMES = weekdayNames('long');

  let mode = $state<'new' | 'join'>('new');
  let label = $state('PC');
  let wake = $state('06:30');
  let bedtime = $state('22:30');
  let freeDays = $state(0b110_0000); // Saturday and Sunday
  let permits = $state(3);
  let permitMinutes = $state(5);
  let goals = $state<Goal[]>([
    { id: 'read-30', title: t('onb.goal_read'), kind: 'key', target_minutes: 30, target_count: 1, methods: methodsFor(30), weekdays: ALL_DAYS },
    { id: 'eat-healthy', title: t('onb.goal_eat'), kind: 'habit', target_minutes: 0, target_count: 2, methods: methodsFor(0), weekdays: ALL_DAYS },
  ]);
  let newGoal = $state('');
  let newMinutes = $state(30);
  let essentials = $state('windows:pfn:5319275A.WhatsAppDesktop_cv1g1gvanyjgm');
  let focusApps = $state('windows:exe:Microsoft Corporation|winword.exe\nwindows:exe:Google LLC|chrome.exe');
  let web = $state('udemy.com\ncoursera.org');
  let qr = $state('');
  let code = $state<string | null>(null);
  let busy = $state(false);

  const lines = (s: string) => s.split('\n').map((l) => l.trim()).filter(Boolean);
  const toggleDay = (i: number) => (freeDays ^= 1 << i);

  function addGoal() {
    const title = newGoal.trim();
    if (!title) return;
    goals.push({
      id: slug(title, goals.map((g) => g.id)), title, kind: 'key', target_minutes: newMinutes,
      target_count: 1, methods: methodsFor(newMinutes), weekdays: ALL_DAYS,
    });
    newGoal = '';
  }

  async function create() {
    busy = true;
    try {
      const config: Config = {
        schedule: { wake: { minutes: toMinutes(wake) }, bedtime: { minutes: toMinutes(bedtime) } },
        permits_per_day: Math.min(10, Math.max(0, permits)),
        permit_minutes: Math.min(60, Math.max(1, permitMinutes)),
        permit_cooldown_minutes: 60,
        emergency_minutes: 15,
        emergency_penalty: 1,
        habit_penalty: true,
        goals,
        free_days: freeDays,
        essential_apps: lines(essentials),
        focus_apps: lines(focusApps),
        web_allowlist: lines(web).map((d) => d.toLowerCase()),
      };
      await call({ cmd: 'create_profile', label: label.trim() || 'PC', config });
      ondone();
    } catch (e) {
      onerror(e);
    } finally {
      busy = false;
    }
  }

  async function join() {
    busy = true;
    try {
      code = await text({ cmd: 'pairing_join', qr: qr.trim(), label: label.trim() || 'PC' });
      // The other device confirms; the service fetches the history and the screen moves on by itself.
      for (let i = 0; i < 100; i++) {
        await new Promise((r) => setTimeout(r, 3000));
        const r = await call({ cmd: 'status' });
        if (r.type === 'status' && r.agent.core) return ondone();
      }
    } catch (e) {
      onerror(e);
    } finally {
      busy = false;
    }
  }
</script>

<main class="onb stack">
  <header class="stack" style="gap:6px">
    <img src="/logo.png" alt="" width="64" height="64" />
    <h1>{t('onb.title')}</h1>
    <p class="muted">{t('onb.subtitle')}</p>
  </header>

  <div class="row" role="tablist" aria-label={t('onb.how_to_start')}>
    <button role="tab" aria-selected={mode === 'new'} class={mode === 'new' ? 'btn-tonal' : 'btn-text'} onclick={() => (mode = 'new')}>{t('onb.create')}</button>
    <button role="tab" aria-selected={mode === 'join'} class={mode === 'join' ? 'btn-tonal' : 'btn-text'} onclick={() => (mode = 'join')}>{t('onb.join')}</button>
  </div>

  <label><span>{t('onb.device_name')}</span><input bind:value={label} maxlength="64" /></label>

  {#if mode === 'new'}
    <section class="card stack" aria-labelledby="h-sleep">
      <h2 id="h-sleep">{t('onb.schedule')}</h2>
      <div class="row">
        <label><span>{t('onb.wake')}</span><input type="time" bind:value={wake} /></label>
        <label><span>{t('onb.bedtime')}</span><input type="time" bind:value={bedtime} /></label>
      </div>
      <fieldset class="days">
        <legend>{t('onb.free_days')} <small class="muted">{t('onb.free_days_hint')}</small></legend>
        {#each DAY_LABELS as d, i}
          <label class="day">
            <input type="checkbox" checked={(freeDays & (1 << i)) !== 0} onchange={() => toggleDay(i)} aria-label={DAY_NAMES[i]} />
            <span>{d}</span>
          </label>
        {/each}
      </fieldset>
      <div class="row">
        <label><span>{t('onb.passes_per_day')}</span><input type="number" min="0" max="10" bind:value={permits} /></label>
        <label><span>{t('onb.minutes_per_pass')}</span><input type="number" min="1" max="60" bind:value={permitMinutes} /></label>
      </div>
      <p class="muted small">{t('onb.anti_impulse')}</p>
    </section>

    <section class="card stack" aria-labelledby="h-goals">
      <h2 id="h-goals">{t('onb.fixed_goals')}</h2>
      <p class="muted small">{t('onb.fixed_goals_hint')}</p>
      <ul class="list">
        {#each goals as g, i (g.id)}
          <li class="row">
            <strong>{g.title}</strong>
            <span class="chip {g.kind === 'key' ? '' : 'neutral'}">{g.kind === 'key' ? t('common.unlocks') : t('common.habit')}</span>
            <span class="muted">{g.target_minutes ? duration(g.target_minutes) : tn('onb.times', g.target_count)}</span>
            <button class="btn-text" onclick={() => goals.splice(i, 1)}>{t('common.remove')}</button>
          </li>
        {/each}
      </ul>
      <div class="row">
        <label class="grow"><span>{t('onb.new_goal')}</span><input bind:value={newGoal} maxlength="80" /></label>
        <label><span>{t('common.minutes')}</span><input type="number" min="0" max="600" step="5" bind:value={newMinutes} /></label>
        <button class="btn-secondary" disabled={!newGoal.trim()} onclick={addGoal}>{t('common.add')}</button>
      </div>
    </section>

    <details class="card">
      <summary>{t('onb.apps_sites')}</summary>
      <div class="stack" style="margin-top:12px">
        <label><span>{t('onb.essential_apps')}</span><textarea rows="2" bind:value={essentials}></textarea></label>
        <label><span>{t('onb.focus_apps')}</span><textarea rows="2" bind:value={focusApps}></textarea></label>
        <label><span>{t('onb.focus_sites')}</span><textarea rows="2" bind:value={web}></textarea></label>
      </div>
    </details>

    <button class="btn-primary" disabled={busy || !goals.some((g) => g.kind === 'key')} onclick={create}>{t('onb.create_profile')}</button>
  {:else if !code}
    <section class="card stack">
      <p>{t('onb.join_instructions')}</p>
      <textarea rows="4" bind:value={qr} aria-label={t('onb.pairing_code')}></textarea>
      <button class="btn-primary" disabled={busy || !qr.trim()} onclick={join}>{t('onb.join_button')}</button>
    </section>
  {:else}
    <section class="card stack" aria-live="polite">
      <p>{t('onb.check_code')}</p>
      <p class="code num">{code}</p>
      <p class="muted">{t('onb.confirm_there')}</p>
    </section>
  {/if}
</main>

<style>
  .onb { max-width: 760px; margin: 0 auto; padding: 32px 16px 48px; }
  .days { border: 0; padding: 0; margin: 0; display: flex; gap: 8px; flex-wrap: wrap; }
  .days legend { margin-bottom: 8px; font-weight: 600; color: var(--muted); font-size: 14px; }
  .day { display: inline-flex; flex-direction: row; align-items: center; gap: 4px; color: var(--text); }
  .day input { min-height: auto; width: 18px; height: 18px; }
  .list { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
  .grow { flex: 1 1 220px; }
  .small { font-size: 14px; }
  .code { font-size: 48px; font-weight: 700; letter-spacing: 0.12em; }
  summary { cursor: pointer; font-weight: 600; }
</style>
