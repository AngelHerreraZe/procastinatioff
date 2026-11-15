<script lang="ts">
  // 08 Foco: la siguiente acción, enorme. Se abre al empezar cualquier meta.
  // - Meta con tiempo → sesión de ENFOQUE (el núcleo mide los minutos).
  // - Meta sin tiempo → declaración con fricción: escribir qué hiciste + esperar 60 s (el texto no se guarda).
  // - Meta con cámara del PC → el verificador corre aislado y sin red; solo devuelve si se cumplió.
  import { canVerifyOnPc, stopVerification, verifyOnPc } from '../lib/native';
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
    onerror: (m: string) => void;
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

  // Lectores de pantalla: solo hitos (cada 5 min y el último minuto), nunca cada segundo.
  $effect(() => {
    if (!declaring && (remaining % 5 === 0 || remaining <= 1)) {
      announced = remaining === 0 ? 'Tiempo cumplido. Ya puedes terminar.' : `Quedan ${remaining} minutos`;
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
      else cameraNote = 'Esta vez no se pudo comprobar. Puedes intentarlo de nuevo o usar otra opción.';
    } catch (e) {
      cameraNote = String(e);
    } finally {
      verifying = false;
    }
  }

  async function declare() {
    try {
      await call({ cmd: 'complete_goal', goal_id: goalId, method: 'honor_declaration', score: 100, minutes: goal?.target_minutes ?? 0 });
      ondeclared();
    } catch (e) {
      onerror(String(e));
    }
  }
</script>

<main class="focus">
  <div class="top row">
    <button class="btn-text" onclick={declaring ? oncancel : onoverview}>{declaring ? 'Volver al día' : 'Ver todo el día'}</button>
    {#if keys.length > 0}<p class="muted num">Paso {stepIndex + 1} de {keys.length}</p>{/if}
  </div>

  <section class="card focus-card stack" aria-labelledby="task">
    <span class="chip now">Ahora</span>
    <h1 id="task">{progress?.title ?? 'Tu meta'}</h1>

    {#if declaring}
      <p class="instruction">Escribe en una frase lo que hiciste. Después de 60 s podrás marcarla como hecha.</p>
      <label>
        <span class="sr-only">Lo que hiciste</span>
        <textarea rows="3" bind:value={note} maxlength="300" placeholder="Hoy he…"></textarea>
        <small class="muted">No se guarda ni se envía: solo cuenta como «autodeclarada».</small>
      </label>
      <button class="btn-primary big" disabled={left > 0 || note.trim().length < 10} onclick={declare}>
        {left > 0 ? `Espera ${left} s` : 'Marcar como hecha'}
      </button>
    {:else}
      <p class="instruction">Siéntate, abre lo que necesitas y sigue. Solo están disponibles las apps de esta tarea.</p>
      <p class="timer num" role="timer" aria-label="Quedan {remaining} minutos">{remaining}<span> min</span></p>
      <div class="bar thick" role="progressbar" aria-label="Progreso" aria-valuemin="0" aria-valuemax={target} aria-valuenow={done}>
        <i style="width:{target ? (100 * done) / target : 0}%"></i>
      </div>
      <p class="muted num">{done} de {target} min</p>
      <button class="btn-primary big" onclick={() => onact({ cmd: 'end_focus' })}>
        {remaining === 0 ? 'Terminar: meta cumplida' : 'Terminar sesión por ahora'}
      </button>
      <p class="sr-only" aria-live="polite">{announced}</p>
    {/if}

    {#if cameraMethod}
      {#if verifying}
        <div class="camera-on" role="status">
          <span><strong>Cámara activa</strong> · no se graba nada · el verificador no tiene Internet</span>
          <button class="btn-secondary" onclick={() => stopVerification()}>Detener</button>
        </div>
      {:else}
        <button class="btn-secondary" onclick={verifyWithCamera}>
          {cameraMethod === 'pose_reps' ? 'Contar repeticiones con la cámara' : 'Comprobar la lectura con la cámara'}
        </button>
      {/if}
      {#if cameraNote}<p class="muted" role="status">{cameraNote}</p>{/if}
    {/if}
  </section>

  <div class="next stack">
    {#if after}<p><strong>Después:</strong> {after.title}</p>{/if}
    {#if habit}<p class="muted"><strong>Sin prisa:</strong> {habit.title} (hábito)</p>{/if}
  </div>

  {#if !declaring}
    <div class="row secondary">
      <button class="btn-tonal" disabled={status.permits_left === 0} onclick={() => onact({ cmd: 'start_pass' })}>
        Permiso ({status.permits_left})
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
