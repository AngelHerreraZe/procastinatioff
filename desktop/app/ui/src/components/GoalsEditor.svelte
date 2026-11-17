<script lang="ts">
  // Fixed goals with how each one is checked and what its focus opens (apps and sites, one per line).
  // Used when creating the profile and in Settings; the core decides when a change applies (relaxing: at
  // the next wake-up). Editing writes straight into the list: a change is never lost for not pressing a button.
  import { ALL_DAYS, CHECKS, checkOf, methodsForCheck, slug, usesMinutes, type Check, type Goal } from '../lib/agent';
  import { duration } from '../lib/format';
  import { t, tn } from '../lib/i18n';
  import { newPlaceAnchor, qrSvg } from '../lib/native';
  import CheckSelect from './CheckSelect.svelte';

  let { goals = $bindable() }: { goals: Goal[] } = $props();

  let editing = $state<number | null>(null);
  let title = $state('');
  let minutes = $state(30);
  let check = $state<Check>('focus');
  let apps = $state('');
  let sites = $state('');
  /** A new place QR: its hash goes in the goal, its SVG is shown once to print. */
  let anchor = $state<{ code: string; hash: string } | null>(null);
  let qr = $state('');

  const lines = (s: string) => [...new Set(s.split('\n').map((l) => l.trim()).filter(Boolean))];
  const hasAnchor = $derived(editing !== null && !!goals[editing]?.anchor);

  function reset() {
    editing = null;
    title = '';
    minutes = 30;
    check = 'focus';
    apps = '';
    sites = '';
    anchor = null;
  }

  function edit(i: number) {
    const g = goals[i];
    editing = i;
    title = g.title;
    minutes = g.target_minutes;
    check = checkOf(g.methods);
    apps = (g.apps ?? []).join('\n');
    sites = (g.sites ?? []).join('\n');
    anchor = null;
  }

  function remove(i: number) {
    goals = goals.filter((_, j) => j !== i);
    reset();
  }

  /** The form as a goal: a new one, or `old` with the form's changes. Place goals keep their anchor. */
  function fromForm(old?: Goal): Goal {
    const m = usesMinutes(check) ? Math.min(600, Math.max(0, Math.round(minutes || 0))) : 0;
    const form = {
      target_minutes: m,
      methods: methodsForCheck(check, m),
      apps: lines(apps),
      sites: lines(sites.toLowerCase()),
      anchor: check === 'place' ? (old?.anchor || anchor?.hash || '') : '',
    };
    if (old) return { ...old, ...form, title: title.trim() || old.title };
    return { id: slug(title.trim(), goals.map((g) => g.id)), title: title.trim(), kind: 'key', target_count: 1, weekdays: ALL_DAYS, ...form };
  }

  function changed() {
    const at = editing;
    if (at !== null) goals = goals.map((g, j) => (j === at ? fromForm(g) : g));
  }

  async function pick(c: Check) {
    check = c;
    if (c === 'place' && !hasAnchor && !anchor) {
      anchor = await newPlaceAnchor();
      qr = await qrSvg(anchor.code);
    }
    changed();
  }

  function add() {
    if (!title.trim()) return;
    goals = [...goals, fromForm()];
    reset();
  }
</script>

<p class="muted small">{t('onb.fixed_goals_hint')}</p>
<ul class="list">
  {#each goals as g, i (g.id)}
    <li class="row">
      <strong>{g.title}</strong>
      <span class="chip {g.kind === 'key' ? '' : 'neutral'}">{g.kind === 'key' ? t('common.unlocks') : t('common.habit')}</span>
      <span class="muted">{g.target_minutes ? duration(g.target_minutes) : tn('onb.times', g.target_count)} · {t(`check.${checkOf(g.methods)}`)}</span>
      <button class="btn-text" aria-pressed={editing === i} onclick={() => edit(i)}>{t('goals.edit')}</button>
      <button class="btn-text" onclick={() => remove(i)}>{t('common.remove')}</button>
      {#if g.apps?.length || g.sites?.length}
        <small class="muted opens">{[...(g.apps ?? []), ...(g.sites ?? [])].join(', ')}</small>
      {/if}
    </li>
  {/each}
</ul>
<div class="row">
  <label class="grow"><span>{t(editing === null ? 'onb.new_goal' : 'goals.name')}</span><input bind:value={() => title, (v) => ((title = v), changed())} maxlength="80" /></label>
  <CheckSelect {title} {check} options={CHECKS} suggest={editing === null} onpick={pick} />
  {#if usesMinutes(check)}
    <label><span>{t('common.minutes')}</span><input type="number" min="0" max="600" step="5" bind:value={() => minutes, (v) => ((minutes = v), changed())} /></label>
  {/if}
</div>
{#if qr}
  <section class="card qr stack" aria-labelledby="qr-title">
    <h3 id="qr-title">{t('goals.place_qr_title')}</h3>
    <!-- Generated locally by the core (qr_svg): no external content -->
    <div class="qr-image" role="img" aria-label={t('goals.place_qr_title')}>{@html qr}</div>
    <p class="muted small">{t('goals.place_qr_note')}</p>
    <div class="row">
      <button class="btn-secondary" onclick={() => window.print()}>{t('goals.print')}</button>
      <button class="btn-text" onclick={() => (qr = '')}>{t('goals.place_qr_saved')}</button>
    </div>
  </section>
{/if}
<label><span>{t('onb.goal_apps')}</span><textarea rows="2" bind:value={() => apps, (v) => ((apps = v), changed())} placeholder="windows:exe:Microsoft Corporation|winword.exe"></textarea></label>
<label><span>{t('onb.goal_sites')}</span><textarea rows="2" bind:value={() => sites, (v) => ((sites = v), changed())} placeholder="udemy.com"></textarea></label>
<div class="row">
  {#if editing === null}
    <button class="btn-secondary" disabled={!title.trim() || (check === 'place' && !anchor)} onclick={add}>{t('onb.add_goal')}</button>
  {:else}
    <button class="btn-secondary" onclick={reset}>{t('goals.done')}</button>
  {/if}
</div>

<style>
  .list { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
  .grow { flex: 1 1 220px; }
  .small { font-size: 14px; }
  .opens { flex-basis: 100%; overflow-wrap: anywhere; }
  [aria-pressed='true'] { text-decoration: underline; }
  .qr-image { max-width: 280px; background: #fff; padding: 8px; border-radius: 12px; }
  .qr-image :global(svg) { width: 100%; height: auto; display: block; }
  /* Printing from here prints only the QR */
  @media print {
    :global(body *) { visibility: hidden; }
    .qr, .qr :global(*) { visibility: visible; }
    .qr { position: absolute; inset: 0 auto auto 0; }
    .qr button { display: none; }
  }
</style>
