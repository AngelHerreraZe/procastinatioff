<script lang="ts">
  // Day plan: opens by itself at the first unlock after bedtime.
  // The configuration's fixed goals are already included; they can be raised, not removed.
  // Once confirmed, the plan can only be tightened (add goals or raise targets) until tomorrow.
  import { untrack } from 'svelte';
  import {
    ALL_DAYS, PLAN_CHECKS, call, methodsForCheck, slug, usesMinutes, type Check, type Goal, type GoalKind, type Status,
  } from '../lib/agent';
  import CheckSelect from '../components/CheckSelect.svelte';
  import { today } from '../lib/format';
  import { t } from '../lib/i18n';

  let {
    goals, status, adding, onerror, ondone, oncancel,
  }: {
    goals: Goal[];
    status: Status;
    adding: boolean;
    onerror: (e: unknown) => void;
    ondone: () => void;
    oncancel: () => void;
  } = $props();

  type Row = Goal & { locked: boolean; min_minutes: number; min_count: number };
  let rows = $state<Row[]>(
    untrack(() => goals.map((g) => ({ ...g, locked: true, min_minutes: g.target_minutes, min_count: g.target_count }))),
  );
  let title = $state('');
  let kind = $state<GoalKind>('key');
  let minutes = $state(30);
  let count = $state(1);
  let check = $state<Check>('focus');
  let busy = $state(false);

  const hasKey = $derived(rows.some((r) => r.kind === 'key'));

  function add() {
    const name = title.trim();
    if (!name) return;
    const mins = usesMinutes(check) ? Math.max(0, Math.min(600, Math.round(minutes))) : 0;
    rows.push({
      id: slug(name, rows.map((r) => r.id)),
      title: name,
      kind,
      target_minutes: mins,
      target_count: Math.max(1, Math.min(10, Math.round(count))),
      methods: methodsForCheck(check, mins),
      weekdays: ALL_DAYS,
      locked: false,
      min_minutes: 0,
      min_count: 1,
    });
    title = '';
  }

  async function confirm() {
    busy = true;
    try {
      const plan: Goal[] = rows.map(({ locked: _l, min_minutes: _m, min_count: _c, ...g }) => ({
        ...g,
        target_minutes: Math.max(g.target_minutes, _m),
        target_count: Math.max(g.target_count, _c),
      }));
      await call({ cmd: 'plan_day', goals: plan });
      ondone();
    } catch (e) {
      onerror(e);
    } finally {
      busy = false;
    }
  }
</script>

<main class="plan stack">
  <header class="stack" style="gap:6px">
    <p class="muted date">{today()}</p>
    <h1>{adding ? t('plan.title_adding') : t('plan.title_morning')}</h1>
    <p class="lead">
      {adding ? t('plan.lead_adding') : t('plan.lead_morning')}
    </p>
  </header>

  <section class="stack" aria-labelledby="list-title" style="gap:8px">
    <h2 id="list-title">{t('common.today_goals')}</h2>
    {#if rows.length === 0}
      <p class="muted">{t('plan.empty')}</p>
    {/if}
    <ul class="list">
      {#each rows as r, i (r.id)}
        <li class="card item">
          <div class="stack" style="gap:4px">
            <div class="row" style="gap:8px">
              <strong>{r.title}</strong>
              <span class="chip {r.kind === 'key' ? '' : 'neutral'}">{r.kind === 'key' ? t('common.unlocks') : t('common.habit')}</span>
              {#if r.locked}<span class="chip neutral">{adding ? t('plan.in_plan') : t('plan.fixed')}</span>{/if}
            </div>
          </div>
          <div class="row" style="gap:8px">
            {#if r.target_minutes > 0 || r.min_minutes > 0}
              <label class="inline"><span class="sr-only">{t('plan.minutes_of', { goal: r.title })}</span>
                <input type="number" min={r.min_minutes} max="600" step="5" bind:value={r.target_minutes} /> {t('plan.unit_min')}
              </label>
            {:else}
              <label class="inline"><span class="sr-only">{t('plan.times_of', { goal: r.title })}</span>
                <input type="number" min={r.min_count} max="10" bind:value={r.target_count} /> {t('plan.unit_times')}
              </label>
            {/if}
            {#if !r.locked}
              <button class="btn-text" onclick={() => rows.splice(i, 1)}>{t('common.remove')}</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  </section>

  <section class="card stack" aria-labelledby="add-title">
    <h3 id="add-title">{t('plan.add_title')}</h3>
    <div class="add">
      <label class="grow"><span>{t('plan.name')}</span>
        <input bind:value={title} maxlength="80" placeholder={t('plan.name_placeholder')} onkeydown={(e) => e.key === 'Enter' && add()} />
      </label>
      <fieldset class="kind">
        <legend class="sr-only">{t('plan.kind')}</legend>
        <label class="radio"><input type="radio" bind:group={kind} value="key" /> {t('common.unlocks')}</label>
        <label class="radio"><input type="radio" bind:group={kind} value="habit" /> {t('common.habit')}</label>
      </fieldset>
      <CheckSelect {title} {check} options={PLAN_CHECKS} suggest onpick={(c) => (check = c)} />
      {#if usesMinutes(check)}
        <label><span>{t('common.minutes')}</span><input type="number" min="0" max="600" step="5" bind:value={minutes} /></label>
      {/if}
      {#if !usesMinutes(check) || minutes === 0}
        <label><span>{t('common.times')}</span><input type="number" min="1" max="10" bind:value={count} /></label>
      {/if}
      <button class="btn-secondary" disabled={!title.trim()} onclick={add}>{t('common.add')}</button>
    </div>
    <p class="muted small">
      {t('plan.note')}
    </p>
  </section>

  <div class="row">
    <button class="btn-primary" disabled={busy || !hasKey} onclick={confirm}>
      {adding ? t('plan.save') : t('plan.start_day')}
    </button>
    {#if adding}<button class="btn-text" onclick={oncancel}>{t('common.cancel')}</button>{/if}
    {#if !hasKey}<p class="muted small">{t('plan.need_key')}</p>{/if}
  </div>
</main>

<style>
  .plan { max-width: 760px; margin: 0 auto; padding: 32px 16px 48px; }
  .date::first-letter { text-transform: uppercase; }
  .lead { font-size: 17px; line-height: 1.5; color: var(--muted); }
  .list { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
  .item { display: flex; justify-content: space-between; align-items: center; gap: 12px; flex-wrap: wrap; padding: 14px 16px; }
  .inline { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); }
  .inline input { width: 84px; }
  .add { display: flex; gap: 12px; align-items: end; flex-wrap: wrap; }
  .add .grow { flex: 1 1 220px; }
  .add input[type='number'] { width: 96px; }
  .kind { border: 0; padding: 0; margin: 0; display: flex; gap: 12px; min-height: 40px; align-items: center; }
  .radio { display: inline-flex; flex-direction: row; align-items: center; gap: 6px; color: var(--text); font-size: 15px; }
  .radio input { min-height: auto; }
  .small { font-size: 14px; }
</style>
