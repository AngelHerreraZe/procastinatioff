<script lang="ts">
  // Fixed goals with what each one's focus opens (apps and sites, one per line). Used when creating the
  // profile and in Settings; the core decides when a change applies (relaxing: at the next wake-up).
  import { ALL_DAYS, methodsFor, slug, type Goal } from '../lib/agent';
  import { duration } from '../lib/format';
  import { t, tn } from '../lib/i18n';

  let { goals = $bindable() }: { goals: Goal[] } = $props();

  let editing = $state<number | null>(null);
  let title = $state('');
  let minutes = $state(30);
  let apps = $state('');
  let sites = $state('');

  const lines = (s: string) => [...new Set(s.split('\n').map((l) => l.trim()).filter(Boolean))];

  function reset() {
    editing = null;
    title = '';
    minutes = 30;
    apps = '';
    sites = '';
  }

  function edit(i: number) {
    const g = goals[i];
    editing = i;
    title = g.title;
    minutes = g.target_minutes;
    apps = (g.apps ?? []).join('\n');
    sites = (g.sites ?? []).join('\n');
  }

  function remove(i: number) {
    goals = goals.filter((_, j) => j !== i);
    reset();
  }

  function save() {
    const name = title.trim();
    if (!name) return;
    const m = Math.min(600, Math.max(0, Math.round(minutes || 0)));
    const own = { apps: lines(apps), sites: lines(sites.toLowerCase()) };
    const at = editing;
    if (at === null) {
      const id = slug(name, goals.map((g) => g.id));
      goals = [...goals, { id, title: name, kind: 'key', target_minutes: m, target_count: 1, methods: methodsFor(m), weekdays: ALL_DAYS, ...own }];
    } else {
      // The methods only change when the goal starts or stops being measured by time.
      goals = goals.map((g, j) =>
        j === at ? { ...g, title: name, target_minutes: m, methods: (m > 0) === (g.target_minutes > 0) ? g.methods : methodsFor(m), ...own } : g,
      );
    }
    reset();
  }
</script>

<p class="muted small">{t('onb.fixed_goals_hint')}</p>
<ul class="list">
  {#each goals as g, i (g.id)}
    <li class="row">
      <strong>{g.title}</strong>
      <span class="chip {g.kind === 'key' ? '' : 'neutral'}">{g.kind === 'key' ? t('common.unlocks') : t('common.habit')}</span>
      <span class="muted">{g.target_minutes ? duration(g.target_minutes) : tn('onb.times', g.target_count)}</span>
      <button class="btn-text" aria-pressed={editing === i} onclick={() => edit(i)}>{t('goals.edit')}</button>
      <button class="btn-text" onclick={() => remove(i)}>{t('common.remove')}</button>
      {#if g.apps?.length || g.sites?.length}
        <small class="muted opens">{[...(g.apps ?? []), ...(g.sites ?? [])].join(', ')}</small>
      {/if}
    </li>
  {/each}
</ul>
<div class="row">
  <label class="grow"><span>{t(editing === null ? 'onb.new_goal' : 'goals.name')}</span><input bind:value={title} maxlength="80" /></label>
  <label><span>{t('common.minutes')}</span><input type="number" min="0" max="600" step="5" bind:value={minutes} /></label>
</div>
<label><span>{t('onb.goal_apps')}</span><textarea rows="2" bind:value={apps} placeholder="windows:exe:Microsoft Corporation|winword.exe"></textarea></label>
<label><span>{t('onb.goal_sites')}</span><textarea rows="2" bind:value={sites} placeholder="udemy.com"></textarea></label>
<div class="row">
  <button class="btn-secondary" disabled={!title.trim()} onclick={save}>{t(editing === null ? 'onb.add_goal' : 'goals.save')}</button>
  {#if editing !== null}<button class="btn-text" onclick={reset}>{t('common.cancel')}</button>{/if}
</div>

<style>
  .list { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
  .grow { flex: 1 1 220px; }
  .small { font-size: 14px; }
  .opens { flex-basis: 100%; overflow-wrap: anywhere; }
  [aria-pressed='true'] { text-decoration: underline; }
</style>
