<script lang="ts">
  // Decides which screen and theme to show from the service status (polled every second).
  import { bringToFront, lockScreen, setShield } from './lib/native';
  import { call, myRole, status, todayGoals, type AgentStatus, type Goal, type Role } from './lib/agent';
  import { errorText, t } from './lib/i18n';
  import Onboarding from './screens/Onboarding.svelte';
  import PlanDay from './screens/PlanDay.svelte';
  import Home from './screens/Home.svelte';
  import Focus from './screens/Focus.svelte';
  import Shield from './screens/Shield.svelte';
  import Pairing from './screens/Pairing.svelte';
  import Family from './screens/Family.svelte';
  import Settings from './screens/Settings.svelte';


  let agent = $state<AgentStatus | null>(null);
  let goals = $state<Goal[]>([]);
  let message = $state('');
  let now = $state(Date.now());
  let view = $state<'home' | 'pair' | 'plan' | 'overview' | 'family' | 'settings'>('home');
  let role = $state<Role | null>(null);
  let declaring = $state<string | null>(null); // untimed goal in progress (declaration with friction)
  let dismissedAt = $state<number | null>(null);
  let planShownFor = 0;
  let shieldOn = false;


  const core = $derived(agent?.core ?? null);
  const st = $derived(core?.state.state);
  const nearSleep = $derived(core?.sleep_at_ms != null && now >= core.sleep_at_ms - 3_600_000);
  const shield = $derived<'sleep' | 'grace' | 'blocked' | null>(
    !core
      ? null
      : st === 'sleep'
        ? 'sleep'
        : agent?.grace_until_ms
          ? 'grace'
          : agent?.blocked_app && (st === 'locked' || st === 'focus') && agent.blocked_at_ms !== dismissedAt
            ? 'blocked'
            : null,
  );
  const inFocus = $derived(st === 'focus' || declaring !== null);
  const needsPlan = $derived(!!core?.needs_plan && st === 'locked');
  // DESIGN.md: Nocturno while sleeping and in the last hour before bedtime; Calma the rest of the day.
  const theme = $derived(st === 'sleep' || nearSleep ? 'nocturno' : 'calma');

  $effect(() => {
    document.documentElement.dataset.theme = theme;
  });

  async function refresh() {
    try {
      const a = await status();
      const g = a.core ? await todayGoals() : [];
      role = a.core ? await myRole() : null;
      // Both at once: the day plan mounts with its goals already loaded.
      agent = a;
      goals = g;
      now = Date.now();
      message = '';
      if (declaring && !goals.some((g) => g.id === declaring)) declaring = null;
      if (st !== 'focus' && view === 'overview' && !declaring) view = 'home';
    } catch (e) {
      message = errorText(e);
    }
  }

  // Full-screen shield (when an app is closed, during SLEEP or during the countdown).
  $effect(() => {
    const want = shield !== null;
    if (want === shieldOn) return;
    shieldOn = want;
    (async () => {
      await setShield(want);
      if (shield === 'sleep') await lockScreen();
    })().catch((e) => (message = errorText(e)));
  });

  // First unlock after bedtime: the window opens by itself with the day plan.
  $effect(() => {
    if (!needsPlan || !core || planShownFor === core.day_start_ms) return;
    planShownFor = core.day_start_ms;
    bringToFront().catch(() => {});
  });

  $effect(() => {
    refresh();
    const id = setInterval(refresh, 1000);
    return () => clearInterval(id);
  });

  async function act(request: Record<string, unknown>) {
    try {
      await call(request);
      await refresh();
    } catch (e) {
      message = errorText(e);
    }
  }

  function start(goal: Goal) {
    view = 'home';
    if (goal.methods.includes('focus_session')) act({ cmd: 'start_focus', goal_id: goal.id });
    else declaring = goal.id;
  }

  const fail = (e: unknown) => (message = errorText(e));
</script>

{#if !agent}
  <main class="center stack">
    <img src="/logo.png" alt="" width="72" height="72" />
    <h1>ProcastinatiOFF</h1>
    <p class="muted" aria-live="polite">{message ? t('app.no_service') : t('app.connecting')}</p>
  </main>
{:else if !core}
  <Onboarding onerror={fail} ondone={refresh} />
{:else if shield}
  <Shield
    reason={shield}
    {agent}
    {goals}
    {now}
    minor={role === 'minor'}
    onstart={start}
    ondismiss={() => (dismissedAt = agent?.blocked_at_ms ?? null)}
    onfamily={() => {
      dismissedAt = agent?.blocked_at_ms ?? null;
      view = 'family';
    }}
    onact={act}
    onerror={fail}
  />
{:else if view === 'family'}
  <Family status={core} onclose={() => (view = 'home')} onerror={fail} />
{:else if view === 'pair'}
  <Pairing onclose={() => (view = 'home')} onerror={fail} />
{:else if view === 'settings'}
  <Settings onclose={() => (view = 'home')} onerror={fail} />
{:else if needsPlan || view === 'plan'}
  <PlanDay
    {goals}
    status={core}
    adding={!needsPlan}
    onerror={fail}
    ondone={() => {
      view = 'home';
      refresh();
    }}
    oncancel={() => (view = 'home')}
  />
{:else if inFocus && view !== 'overview'}
  <Focus
    status={core}
    {goals}
    goalId={st === 'focus' && core.state.state === 'focus' ? core.state.goal_id : declaring!}
    declaring={st !== 'focus'}
    onact={act}
    onerror={fail}
    onoverview={() => (view = 'overview')}
    ondeclared={() => {
      declaring = null;
      refresh();
    }}
    oncancel={() => (declaring = null)}
  />
{:else}
  <Home
    {agent}
    status={core}
    {goals}
    {now}
    focusActive={inFocus}
    onstart={start}
    onact={act}
    onerror={fail}
    onpair={() => (view = 'pair')}
    onfamily={role === 'minor' ? () => (view = 'family') : null}
    onplan={() => (view = 'plan')}
    onsettings={() => (view = 'settings')}
    onback={() => (view = 'home')}
  />
{/if}

{#if message}
  <p class="toast error" role="alert">{message}</p>
{/if}

<style>
  .center { min-height: 100vh; display: grid; place-content: center; justify-items: center; text-align: center; padding: 24px; }
  .toast {
    position: fixed; left: 50%; bottom: 16px; transform: translateX(-50%); max-width: min(560px, calc(100vw - 32px));
    background: var(--surface); border: 1px solid var(--danger); border-radius: 12px; padding: 10px 16px;
  }
</style>
