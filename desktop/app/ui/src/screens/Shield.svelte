<script lang="ts">
  // Full-screen shield. Always explains the block and the next step; never blames.
  import type { AgentStatus, Goal } from '../lib/agent';
  import { t, tn } from '../lib/i18n';
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
    onerror: (e: unknown) => void;
  } = $props();

  const status = $derived(agent.core!);
  const pending = $derived(status.goals.filter((g) => g.kind === 'key' && !g.completed));
  const next = $derived(pending[0]);
  const nextGoal = $derived(next ? goals.find((g) => g.id === next.goal_id) : undefined);
  const focusing = $derived(status.state.state === 'focus');
  // A positive reminder, a different one each time the shield appears (shield.cheer_1…5).
  const cheer = t(`shield.cheer_${1 + Math.floor(Math.random() * 5)}`);
</script>

<main class="shield" aria-labelledby="shield-title">
  <section class="card panel stack">
    {#if reason === 'sleep'}
      <div class="icon"><Icon name="moon" size={28} /></div>
      <h1 id="shield-title">{t('shield.sleep_title')}</h1>
      <p class="lead">{t('shield.sleep_lead', { time: hhmm(status.next_wake_ms) })}</p>
    {:else if reason === 'grace'}
      <div class="icon"><Icon name="clock" size={28} /></div>
      <h1 id="shield-title">{t('shield.grace_title')}</h1>
      <p class="lead num" role="alert">
        <strong>{t('shield.grace_lead', { seconds: secondsUntil(agent.grace_until_ms ?? now, now) })}</strong>
      </p>
      <p class="muted">
        {focusing
          ? t('shield.focus_only')
          : pending.length > 0
            ? tn('shield.goals_left', pending.length)
            : t('shield.set_goals')}
      </p>
    {:else}
      <div class="icon"><Icon name="lock" size={28} /></div>
      <h1 id="shield-title">{t('shield.closed_title', { app: agent.blocked_app ?? '' })}</h1>
      <p class="lead">
        {focusing
          ? t('shield.focus_only')
          : next
            ? t('shield.not_done', { goal: next.title })
            : t('shield.set_goals_unlock')}
      </p>
      <div class="row">
        {#if !focusing && nextGoal}
          <button class="btn-primary" onclick={() => { ondismiss(); onstart(nextGoal); }}>{t('shield.continue', { goal: nextGoal.title })}</button>
        {/if}
        {#if status.permits_left > 0}
          <button class="btn-secondary" onclick={() => onact({ cmd: 'start_pass' })}>{t('shield.use_pass', { n: status.permits_left })}</button>
        {/if}
        {#if minor}
          <button class="btn-secondary" onclick={onfamily}>{t('shield.ask_guardian')}</button>
        {/if}
        <button class="btn-text" onclick={ondismiss}>{t('shield.ok')}</button>
      </div>
    {/if}
    {#if reason !== 'sleep'}<p class="cheer">{cheer}</p>{/if}
    <Emergency {onerror} />
  </section>
</main>

<style>
  .shield { min-height: 100vh; display: grid; place-items: center; padding: 24px 16px; background: var(--bg); }
  .panel { max-width: 640px; width: 100%; padding: 36px; gap: 18px; }
  .icon { width: 56px; height: 56px; border-radius: 50%; background: var(--accent-soft); color: var(--accent); display: grid; place-items: center; }
  .lead { font-size: 18px; line-height: 1.5; }
  .cheer { color: var(--accent); font-weight: 600; }
</style>
