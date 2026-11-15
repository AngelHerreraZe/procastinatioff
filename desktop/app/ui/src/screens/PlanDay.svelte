<script lang="ts">
  // Plan del día: se abre solo al primer desbloqueo tras la hora de dormir.
  // Las metas fijas de la configuración ya vienen incluidas; se pueden subir, no quitar.
  // Tras confirmar, el plan solo se puede endurecer (añadir metas o subir objetivos) hasta mañana.
  import { untrack } from 'svelte';
  import {
    ALL_DAYS, CHECKS, call, methodsForCheck, slug, usesMinutes, type Check, type Goal, type GoalKind, type Status,
  } from '../lib/agent';
  import { today } from '../lib/format';

  let {
    goals, status, adding, onerror, ondone, oncancel,
  }: {
    goals: Goal[];
    status: Status;
    adding: boolean;
    onerror: (m: string) => void;
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
  let check = $state<Check>('enfoque');
  let busy = $state(false);

  const hasKey = $derived(rows.some((r) => r.kind === 'key'));

  function add() {
    const t = title.trim();
    if (!t) return;
    const mins = usesMinutes(check) ? Math.max(0, Math.min(600, Math.round(minutes))) : 0;
    rows.push({
      id: slug(t, rows.map((r) => r.id)),
      title: t,
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
      onerror(String(e));
    } finally {
      busy = false;
    }
  }
</script>

<main class="plan stack">
  <header class="stack" style="gap:6px">
    <p class="muted date">{today()}</p>
    <h1>{adding ? 'Añadir metas a hoy' : 'Buenos días'}</h1>
    <p class="lead">
      {adding
        ? 'Puedes añadir metas o subir objetivos. Lo que ya está no se puede quitar hasta mañana.'
        : 'Define tus metas de hoy. Al confirmarlas quedan fijas hasta mañana: después solo podrás añadir o subirlas.'}
    </p>
  </header>

  <section class="stack" aria-labelledby="list-title" style="gap:8px">
    <h2 id="list-title">Metas de hoy</h2>
    {#if rows.length === 0}
      <p class="muted">Aún no hay metas. Añade al menos una que desbloquee.</p>
    {/if}
    <ul class="list">
      {#each rows as r, i (r.id)}
        <li class="card item">
          <div class="stack" style="gap:4px">
            <div class="row" style="gap:8px">
              <strong>{r.title}</strong>
              <span class="chip {r.kind === 'key' ? '' : 'neutral'}">{r.kind === 'key' ? 'Desbloquea' : 'Hábito'}</span>
              {#if r.locked}<span class="chip neutral">{adding ? 'En el plan' : 'Fija'}</span>{/if}
            </div>
          </div>
          <div class="row" style="gap:8px">
            {#if r.target_minutes > 0 || r.min_minutes > 0}
              <label class="inline"><span class="sr-only">Minutos de {r.title}</span>
                <input type="number" min={r.min_minutes} max="600" step="5" bind:value={r.target_minutes} /> min
              </label>
            {:else}
              <label class="inline"><span class="sr-only">Veces de {r.title}</span>
                <input type="number" min={r.min_count} max="10" bind:value={r.target_count} /> veces
              </label>
            {/if}
            {#if !r.locked}
              <button class="btn-text" onclick={() => rows.splice(i, 1)}>Quitar</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  </section>

  <section class="card stack" aria-labelledby="add-title">
    <h3 id="add-title">Añadir una meta</h3>
    <div class="add">
      <label class="grow"><span>Nombre</span>
        <input bind:value={title} maxlength="80" placeholder="Ej.: Estudiar cálculo" onkeydown={(e) => e.key === 'Enter' && add()} />
      </label>
      <fieldset class="kind">
        <legend class="sr-only">Tipo</legend>
        <label class="radio"><input type="radio" bind:group={kind} value="key" /> Desbloquea</label>
        <label class="radio"><input type="radio" bind:group={kind} value="habit" /> Hábito</label>
      </fieldset>
      <label><span>Cómo se comprueba</span>
        <select bind:value={check}>
          {#each CHECKS as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select>
      </label>
      {#if usesMinutes(check)}
        <label><span>Minutos</span><input type="number" min="0" max="600" step="5" bind:value={minutes} /></label>
      {/if}
      {#if !usesMinutes(check) || minutes === 0}
        <label><span>Veces</span><input type="number" min="1" max="10" bind:value={count} /></label>
      {/if}
      <button class="btn-secondary" disabled={!title.trim()} onclick={add}>Añadir</button>
    </div>
    <p class="muted small">
      Toda meta se puede marcar como hecha con una espera de 60 s. La cámara nunca graba: solo cuenta si se cumplió.
    </p>
  </section>

  <div class="row">
    <button class="btn-primary" disabled={busy || !hasKey} onclick={confirm}>
      {adding ? 'Guardar cambios' : 'Empezar el día'}
    </button>
    {#if adding}<button class="btn-text" onclick={oncancel}>Cancelar</button>{/if}
    {#if !hasKey}<p class="muted small">Necesitas al menos una meta que desbloquee.</p>{/if}
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
