<script lang="ts">
  // Vista del día (01 Calma · Windows): barra lateral, estado con la acción principal integrada y resumen.
  import {
    call, devices, type AgentStatus, type Config, type DeviceSeen, type Goal, type GoalProgress, type Status,
  } from '../lib/agent';
  import { duration, hhmm, minutesUntil, today } from '../lib/format';
  import Icon from '../components/Icon.svelte';
  import Emergency from '../components/Emergency.svelte';

  let {
    agent, status, goals, now, focusActive, onstart, onact, onerror, onpair, onplan, onback, onfamily,
  }: {
    agent: AgentStatus;
    status: Status;
    goals: Goal[];
    now: number;
    focusActive: boolean;
    onstart: (g: Goal) => void;
    onact: (r: Record<string, unknown>) => void;
    onerror: (m: string) => void;
    onpair: () => void;
    onplan: () => void;
    onback: () => void;
    /** Solo en el PC de un menor: pedir al tutor y "Lo que ve tu tutor". */
    onfamily: (() => void) | null;
  } = $props();

  let cfg = $state<Config | null>(null);

  // Latidos cruzados: avisar si otro dispositivo no reporta en más de 24 h (de noche se apagan; por eso no menos).
  const SILENT_MS = 24 * 3_600_000;
  let devs = $state<DeviceSeen[]>([]);
  $effect(() => {
    const load = () => devices().then((d) => (devs = d)).catch(() => {});
    load();
    const id = setInterval(load, 60_000);
    return () => clearInterval(id);
  });
  const silent = $derived(devs.filter((d) => !d.is_me && d.last_seen_ms > 0 && now - d.last_seen_ms > SILENT_MS));
  const ago = (ms: number) => {
    const h = Math.floor((now - ms) / 3_600_000);
    return h < 1 ? 'hace menos de 1 h' : h < 48 ? `hace ${h} h` : `hace ${Math.floor(h / 24)} días`;
  };
  $effect(() => {
    call({ cmd: 'active_config' })
      .then((r) => {
        if (r.type === 'config') cfg = r.config;
      })
      .catch(() => {});
  });

  const st = $derived(status.state);
  const keys = $derived(status.goals.filter((g) => g.kind === 'key'));
  const pending = $derived(keys.filter((g) => !g.completed));
  const next = $derived(pending[0] ?? status.goals.find((g) => !g.completed) ?? null);
  const nextGoal = $derived(next ? goals.find((g) => g.id === next.goal_id) : undefined);
  const canWork = $derived(st.state === 'locked' || st.state === 'free');

  const title = $derived.by(() => {
    switch (st.state) {
      case 'locked': return `Bloqueado · ${pending.length === 1 ? 'falta 1 meta' : `faltan ${pending.length} metas`}`;
      case 'pass': return `Permiso activo · quedan ${minutesUntil(st.until_ms, now)} min`;
      case 'emergency': return `Emergencia · quedan ${minutesUntil(st.until_ms, now)} min`;
      case 'focus': return 'Enfoque en marcha';
      case 'free': return status.free_day ? 'Día libre' : 'Libre';
      default: return '';
    }
  });

  const sentence = $derived.by(() => {
    switch (st.state) {
      case 'locked':
        return next ? `Completa ${pending.length === 1 ? 'la meta que falta' : `${pending.length} metas`} para desbloquear. Sigue con «${next.title}».` : '';
      case 'pass': return 'Cuando termine, vuelve el bloqueo.';
      case 'emergency': return 'Todo está disponible. Queda registrado.';
      case 'focus': return 'Tu sesión sigue contando mientras miras el día.';
      case 'free':
        if (status.free_day) return 'Hoy no hay metas ni bloqueo en ningún dispositivo.';
        return status.sleep_at_ms ? `Libre hasta las ${hhmm(status.sleep_at_ms)}.` : 'Libre hasta mañana.';
      default: return '';
    }
  });

  function value(g: GoalProgress): { done: number; total: number; text: string } {
    if (g.target_minutes > 0) {
      const done = g.completed ? g.target_minutes : Math.min(g.focus_minutes, g.target_minutes);
      return { done, total: g.target_minutes, text: `${done} de ${g.target_minutes} min` };
    }
    return { done: g.done_count, total: g.target_count, text: `${g.done_count} de ${g.target_count}` };
  }

  function actionLabel(g: GoalProgress): string {
    const def = goals.find((d) => d.id === g.goal_id);
    if (def?.methods.includes('focus_session')) return g.focus_minutes > 0 ? 'Continuar' : 'Empezar';
    return 'Marcar como hecha';
  }

  function primaryLabel(g: GoalProgress): string {
    const left = g.target_minutes > 0 ? ` · faltan ${duration(Math.max(0, g.target_minutes - g.focus_minutes))}` : '';
    return `${actionLabel(g)}: ${g.title}${left}`;
  }

  const syncText = $derived(
    agent.synced_at_ms ? `Sincronizado hace ${Math.max(0, Math.round((now - agent.synced_at_ms) / 60_000))} min` : 'Sin sincronizar todavía',
  );
</script>

<div class="layout">
  <nav class="side" aria-label="Secciones">
    <p class="brand">ProcastinaciOFF</p>
    <button class="nav-item active" aria-current="page"><Icon name="home" /> Hoy</button>
    {#if !status.free_day && !status.needs_plan}
      <button class="nav-item" onclick={onplan}><Icon name="plus" /> Añadir meta a hoy</button>
    {/if}
    {#if onfamily}
      <button class="nav-item" onclick={onfamily}><Icon name="target" /> Tu tutor</button>
    {:else}
      <button class="nav-item" onclick={onpair}><Icon name="link" /> Vincular dispositivo</button>
    {/if}
    <p class="side-note muted">Cerrar esta ventana no detiene la protección.</p>
  </nav>

  <main class="main stack">
    <header>
      <p class="muted date">{today()}</p>
      <h1>Hoy</h1>
    </header>

    {#each silent as d (d.device_id)}
      <p class="card silent" role="status">
        <strong>«{d.label}» no reporta {ago(d.last_seen_ms)}.</strong>
        Si lo apagaste, no pasa nada. Si no, revisa que la app siga activa en ese dispositivo.
      </p>
    {/each}

    <section class="card banner" aria-labelledby="state-title">
      <div class="badge" aria-hidden="true">
        <Icon name={st.state === 'locked' ? 'lock' : st.state === 'free' ? (status.free_day ? 'sun' : 'unlock') : 'clock'} size={22} />
      </div>
      <div class="stack" style="gap:8px">
        <h2 id="state-title">{title}</h2>
        <p class="muted">{sentence}</p>
        <div class="row" style="gap:8px">
          {#if status.sleep_at_ms}<span class="chip"><Icon name="moon" size={14} /> Dormir a las {hhmm(status.sleep_at_ms)}</span>{/if}
          {#if !status.free_day}<span class="chip neutral num">{status.permits_left} {status.permits_left === 1 ? 'permiso disponible' : 'permisos disponibles'}</span>{/if}
        </div>
      </div>
      <div class="banner-action">
        {#if focusActive}
          <button class="btn-primary" onclick={onback}>Volver a la tarea</button>
        {:else if canWork && next && nextGoal}
          <button class="btn-primary" onclick={() => onstart(nextGoal)}>{primaryLabel(next)}</button>
        {/if}
      </div>
    </section>

    {#if status.goals.length > 0}
      <section class="stack" aria-labelledby="goals-title" style="gap:8px">
        <h3 id="goals-title">Metas de hoy</h3>
        <ul class="goals">
          {#each status.goals as g (g.goal_id)}
            {@const v = value(g)}
            {@const def = goals.find((d) => d.id === g.goal_id)}
            <li class="card goal">
              <div class="goal-icon" aria-hidden="true"><Icon name={g.completed ? 'check' : g.kind === 'key' ? 'target' : 'leaf'} /></div>
              <div class="goal-body">
                <div class="row" style="gap:8px">
                  <span class="goal-name">{g.title}</span>
                  <span class="chip {g.kind === 'key' ? '' : 'neutral'}">{g.kind === 'key' ? 'Desbloquea' : 'Hábito'}</span>
                </div>
                <div class="bar" role="progressbar" aria-label={g.title} aria-valuemin="0" aria-valuemax={v.total} aria-valuenow={v.done}>
                  <i style="width:{v.total ? (100 * v.done) / v.total : 0}%"></i>
                </div>
                <p class="muted num small">{g.completed ? 'Hecha' : v.text}</p>
              </div>
              {#if !g.completed && canWork && def}
                <button class="btn-text" onclick={() => onstart(def)}>{actionLabel(g)}</button>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if st.state === 'locked' || st.state === 'focus'}
      <div class="row">
        <button class="btn-secondary" disabled={status.permits_left === 0} onclick={() => onact({ cmd: 'start_pass' })}>
          Usar un permiso{cfg ? ` de ${cfg.permit_minutes} min` : ''}
        </button>
        <Emergency minutes={cfg?.emergency_minutes} penalty={cfg?.emergency_penalty} {onerror} />
      </div>
    {/if}

    <footer class="muted small row" style="gap:6px"><Icon name="sync" size={16} /> {syncText}</footer>
  </main>

  <aside class="aside stack" aria-label="Resumen">
    <section class="card stack" style="gap:8px">
      <h3>Resumen</h3>
      {#if status.free_day}
        <p>Hoy es día libre.</p>
      {:else}
        <p class="num">{keys.length - pending.length} de {keys.length} metas que desbloquean</p>
        <p class="num">{status.permits_left} {status.permits_left === 1 ? 'permiso' : 'permisos'} hoy</p>
      {/if}
      <p>{status.sleep_at_ms ? `Dormir a las ${hhmm(status.sleep_at_ms)}` : 'Esta noche no hay hora de dormir'}</p>
      <p>Despiertas a las {hhmm(status.next_wake_ms)}</p>
    </section>
    {#if devs.length > 1}
      <section class="card stack" style="gap:8px">
        <h3>Dispositivos</h3>
        {#each devs as d (d.device_id)}
          <p>{d.label}{d.is_me ? ' (este)' : ''} <span class="muted small">· {d.last_seen_ms ? ago(d.last_seen_ms) : 'sin datos'}</span></p>
        {/each}
      </section>
    {/if}
  </aside>
</div>

<style>
  .layout { display: grid; grid-template-columns: 248px minmax(0, 1fr) 280px; min-height: 100vh; }
  .side { background: var(--surface-2); border-right: 1px solid var(--border); padding: 20px 12px; display: grid; align-content: start; gap: 4px; }
  .brand { font-weight: 700; padding: 0 12px 16px; }
  .nav-item {
    display: flex; align-items: center; gap: 10px; width: 100%; background: none; color: var(--text);
    border-radius: 10px; padding: 0 12px; text-align: left; border-left: 3px solid transparent;
  }
  .nav-item.active { background: var(--accent-soft); border-left-color: var(--accent); font-weight: 600; }
  .side-note { font-size: 13px; padding: 16px 12px 0; }
  .main { padding: 28px 32px; align-content: start; max-width: 860px; }
  .date::first-letter { text-transform: uppercase; }
  .aside { padding: 28px 24px 28px 0; align-content: start; }
  .banner { display: grid; grid-template-columns: 44px 1fr auto; gap: 16px; align-items: center; background: var(--accent-soft); border-color: transparent; }
  .banner-action { align-self: end; }
  .badge { width: 44px; height: 44px; border-radius: 50%; background: var(--surface); color: var(--accent); display: grid; place-items: center; }
  .goals { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
  .goal { display: grid; grid-template-columns: 40px 1fr auto; gap: 14px; align-items: center; padding: 14px 16px; }
  .goal-icon { width: 40px; height: 40px; border-radius: 10px; background: var(--surface-2); display: grid; place-items: center; color: var(--accent); }
  .goal-body { display: grid; gap: 6px; }
  .goal-name { font-weight: 600; }
  .small { font-size: 14px; line-height: 20px; }
  .silent { border-color: var(--accent); }
  @media (max-width: 1000px) {
    .layout { grid-template-columns: 1fr; }
    .side { grid-auto-flow: column; justify-content: start; border-right: 0; border-bottom: 1px solid var(--border); overflow-x: auto; }
    .brand, .side-note { display: none; }
    .aside { padding: 0 16px 24px; }
    .main { padding: 20px 16px; }
    .banner { grid-template-columns: 44px 1fr; }
    .banner-action { grid-column: 1 / -1; }
  }
</style>
