<script lang="ts">
  // Escudo a pantalla completa. Siempre explica el bloqueo y el siguiente paso; nunca culpa.
  import type { AgentStatus, Goal } from '../lib/agent';
  import { hhmm, secondsUntil } from '../lib/format';
  import Icon from '../components/Icon.svelte';
  import Emergency from '../components/Emergency.svelte';

  let {
    reason, agent, goals, now, minor, onstart, ondismiss, onfamily, onact, onerror,
  }: {
    reason: 'sleep' | 'grace' | 'blocked';
    agent: AgentStatus;
    goals: Goal[];
    now: number;
    minor: boolean;
    onstart: (g: Goal) => void;
    ondismiss: () => void;
    onfamily: () => void;
    onact: (r: Record<string, unknown>) => void;
    onerror: (m: string) => void;
  } = $props();

  const status = $derived(agent.core!);
  const pending = $derived(status.goals.filter((g) => g.kind === 'key' && !g.completed));
  const next = $derived(pending[0]);
  const nextGoal = $derived(next ? goals.find((g) => g.id === next.goal_id) : undefined);
  const focusing = $derived(status.state.state === 'focus');
</script>

<main class="shield" aria-labelledby="shield-title">
  <section class="card panel stack">
    {#if reason === 'sleep'}
      <div class="icon"><Icon name="moon" size={28} /></div>
      <h1 id="shield-title">Hora de dormir</h1>
      <p class="lead">Las llamadas siguen funcionando. Despiertas a las {hhmm(status.next_wake_ms)}.</p>
    {:else if reason === 'grace'}
      <div class="icon"><Icon name="clock" size={28} /></div>
      <h1 id="shield-title">Guarda tu trabajo</h1>
      <p class="lead" role="alert">
        En <strong class="num">{secondsUntil(agent.grace_until_ms ?? now, now)} s</strong> se cerrarán las apps que no están
        permitidas ahora.
      </p>
      <p class="muted">
        {focusing
          ? 'Durante el enfoque solo están disponibles las apps de tu tarea.'
          : pending.length > 0
            ? `Aún ${pending.length === 1 ? 'falta 1 meta' : `faltan ${pending.length} metas`} para desbloquear.`
            : 'Define tus metas de hoy para empezar.'}
      </p>
    {:else}
      <div class="icon"><Icon name="lock" size={28} /></div>
      <h1 id="shield-title">Se cerró {agent.blocked_app}</h1>
      <p class="lead">
        {focusing
          ? 'Durante el enfoque solo están disponibles las apps de tu tarea.'
          : next
            ? `Aún no cumples tus metas de hoy. Sigue con «${next.title}».`
            : 'Define tus metas de hoy para desbloquear.'}
      </p>
      <div class="row">
        {#if !focusing && nextGoal}
          <button class="btn-primary" onclick={() => { ondismiss(); onstart(nextGoal); }}>Continuar: {nextGoal.title}</button>
        {/if}
        {#if status.permits_left > 0}
          <button class="btn-secondary" onclick={() => onact({ cmd: 'start_pass' })}>Usar un permiso ({status.permits_left})</button>
        {/if}
        {#if minor}
          <button class="btn-secondary" onclick={onfamily}>Pedir a tu tutor</button>
        {/if}
        <button class="btn-text" onclick={ondismiss}>Entendido</button>
      </div>
    {/if}
    <Emergency {onerror} />
  </section>
</main>

<style>
  .shield { min-height: 100vh; display: grid; place-items: center; padding: 24px 16px; background: var(--bg); }
  .panel { max-width: 640px; width: 100%; padding: 36px; gap: 18px; }
  .icon { width: 56px; height: 56px; border-radius: 50%; background: var(--accent-soft); color: var(--accent); display: grid; place-items: center; }
  .lead { font-size: 18px; line-height: 1.5; }
</style>
