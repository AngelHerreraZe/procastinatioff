<script lang="ts">
  // 08 Focus: the next action, huge. Opens when starting any goal.
  // - Timed goal → FOCUS session (the core measures the minutes).
  // - Untimed goal → declaration with friction: write what you did + wait 60 s (the text is not saved).
  // - Goal with the PC camera → the verifier runs sandboxed without network; it only returns whether it was done.
  import { canVerifyOnPc, stopVerification, verifyOnPc } from '../lib/native';
  import { errorText, t, tn } from '../lib/i18n';
  import { PC_CAMERA, call, type Goal, type Status } from '../lib/agent';
  import Emergency from '../components/Emergency.svelte';

  let {
    status, goals, goalId, declaring, onact, onerror, onoverview, ondeclared, oncancel,
  }: {
    status: Status;
    goals: Goal[];
    goalId: string;
    declaring: boolean;
    onact: (r: Record<string, unknown>) => void;
    onerror: (e: unknown) => void;
    onoverview: () => void;
    ondeclared: () => void;
    oncancel: () => void;
  } = $props();

  const WAIT = 60;
  let note = $state('');
  let left = $state(WAIT);
  let announced = $state('');

  const progress = $derived(status.goals.find((g) => g.goal_id === goalId));
  const goal = $derived(goals.find((g) => g.id === goalId));
  const keys = $derived(status.goals.filter((g) => g.kind === 'key'));
  const stepIndex = $derived(Math.max(0, keys.findIndex((g) => g.goal_id === goalId)));
  const after = $derived(keys.find((g) => !g.completed && g.goal_id !== goalId));
  const habit = $derived(status.goals.find((g) => g.kind === 'habit' && !g.completed && g.goal_id !== goalId));
  const target = $derived(progress?.target_minutes ?? 0);
  const done = $derived(Math.min(progress?.focus_minutes ?? 0, target));
  const remaining = $derived(Math.max(0, target - done));

  $effect(() => {
    if (!declaring) return;
    left = WAIT;
    const id = setInterval(() => (left = Math.max(0, left - 1)), 1000);
    return () => clearInterval(id);
  });

  // Screen readers: only milestones (every 5 min and the last minute), never every second.
  $effect(() => {
    if (!declaring && (remaining % 5 === 0 || remaining <= 1)) {
      announced = remaining === 0 ? t('focus.time_up') : tn('focus.minutes_left', remaining);
    }
  });

  const cameraMethod = $derived(canVerifyOnPc ? goal?.methods.find((m) => PC_CAMERA.includes(m)) : undefined);
  let verifying = $state(false);
  let cameraNote = $state('');

  async function verifyWithCamera() {
    verifying = true;
    cameraNote = '';
    try {
      const approved = await verifyOnPc(goalId, cameraMethod!, goal?.target_minutes ?? 0);
      if (approved) ondeclared();
      else cameraNote = t('verify.not_confirmed');
    } catch (e) {
      cameraNote = errorText(e);
    } finally {
      verifying = false;
    }
  }

  async function declare() {
    try {
      await call({ cmd: 'complete_goal', goal_id: goalId, method: 'honor_declaration', score: 100, minutes: goal?.target_minutes ?? 0 });
      ondeclared();
    } catch (e) {
      onerror(e);
    }
  }
</script>

<main class="focus">
  <div class="top row">
    <button class="btn-text" onclick={declaring ? oncancel : onoverview}>{declaring ? t('focus.back_to_day') : t('focus.see_day')}</button>
    {#if keys.length > 0}<p class="muted num">{t('focus.step', { n: stepIndex + 1, total: keys.length })}</p>{/if}
  </div>

  <section class="card focus-card stack" aria-labelledby="task">
    <span class="chip now">{t('focus.now')}</span>
    <h1 id="task">{progress?.title ?? t('focus.your_goal')}</h1>

    {#if declaring}
      <p class="instruction">{t('focus.declare_instruction')}</p>
      <label>
        <span class="sr-only">{t('focus.what_you_did')}</span>
        <textarea rows="3" bind:value={note} maxlength="300" placeholder={t('focus.declare_placeholder')}></textarea>
        <small class="muted">{t('focus.declare_note')}</small>
      </label>
      <button class="btn-primary big" disabled={left > 0 || note.trim().length < 10} onclick={declare}>
        {left > 0 ? t('common.wait_seconds', { n: left }) : t('common.mark_done')}
      </button>
    {:else}
      <p class="instruction">{t('focus.instruction')}</p>
      <p class="timer num" role="timer" aria-label={tn('focus.minutes_left', remaining)}>{remaining}<span> {t('plan.unit_min')}</span></p>
      <div class="bar thick" role="progressbar" aria-label={t('focus.progress')} aria-valuemin="0" aria-valuemax={target} aria-valuenow={done}>
        <i style="width:{target ? (100 * done) / target : 0}%"></i>
      </div>
      <p class="muted num">{t('home.progress_minutes', { done, total: target })}</p>
      <button class="btn-primary big" onclick={() => onact({ cmd: 'end_focus' })}>
        {remaining === 0 ? t('focus.finish_done') : t('focus.finish_for_now')}
      </button>
      <p class="sr-only" aria-live="polite">{announced}</p>
    {/if}

    {#if cameraMethod}
      {#if verifying}
        <div class="camera-on" role="status">
          <span><strong>{t('focus.camera_on')}</strong> · {t('focus.camera_note')}</span>
          <button class="btn-secondary" onclick={() => stopVerification()}>{t('focus.stop')}</button>
        </div>
      {:else}
        <button class="btn-secondary" onclick={verifyWithCamera}>
          {cameraMethod === 'pose_reps' ? t('focus.count_reps') : t('focus.check_reading')}
        </button>
      {/if}
      {#if cameraNote}<p class="muted" role="status">{cameraNote}</p>{/if}
    {/if}
  </section>

  <div class="next stack">
    {#if after}<p><strong>{t('focus.after')}</strong> {after.title}</p>{/if}
    {#if habit}<p class="muted"><strong>{t('focus.no_rush')}</strong> {t('focus.habit_suffix', { goal: habit.title })}</p>{/if}
  </div>

  {#if !declaring}
    <div class="row secondary">
      <button class="btn-tonal" disabled={status.permits_left === 0} onclick={() => onact({ cmd: 'start_pass' })}>
        {t('focus.pass_count', { n: status.permits_left })}
      </button>
      <Emergency {onerror} />
    </div>
  {/if}
</main>

<style>
  .focus { max-width: 760px; margin: 0 auto; padding: 24px 16px 48px; display: grid; gap: 20px; }
  .top { justify-content: space-between; }
  .focus-card { padding: 36px; gap: 18px; }
  .now { justify-self: start; }
  h1 { font-size: clamp(34px, 5vw, 48px); line-height: 1.15; }
  .instruction { font-size: 19px; line-height: 1.45; }
  .timer { font-size: clamp(72px, 14vw, 136px); line-height: 1; font-weight: 600; margin: 8px 0 0; }
  .timer span { font-size: 0.3em; font-weight: 500; color: var(--muted); }
  .thick { height: 14px; }
  .big { min-height: 64px; font-size: 18px; justify-self: stretch; }
  .next { gap: 6px; font-size: 16px; }
  .secondary { gap: 8px; }
  .camera-on {
    display: flex; gap: 12px; align-items: center; justify-content: space-between; flex-wrap: wrap;
    padding: 12px 16px; border-radius: 16px; background: var(--accent-soft); color: var(--text);
  }
</style>
