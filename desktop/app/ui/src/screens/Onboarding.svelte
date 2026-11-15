<script lang="ts">
  // Alta: crear el perfil en este equipo o unirse a uno existente con el código de otro dispositivo.
  import { ALL_DAYS, DAY_LABELS, DAY_NAMES, call, methodsFor, slug, text, type Config, type Goal } from '../lib/agent';
  import { duration, toMinutes } from '../lib/format';

  let { onerror, ondone }: { onerror: (m: string) => void; ondone: () => void } = $props();

  let mode = $state<'new' | 'join'>('new');
  let label = $state('PC');
  let wake = $state('06:30');
  let bedtime = $state('22:30');
  let freeDays = $state(0b110_0000); // sábado y domingo
  let permits = $state(3);
  let permitMinutes = $state(5);
  let goals = $state<Goal[]>([
    { id: 'leer-30', title: 'Leer 30 minutos', kind: 'key', target_minutes: 30, target_count: 1, methods: methodsFor(30), weekdays: ALL_DAYS },
    { id: 'comer-sano', title: 'Comer sano', kind: 'habit', target_minutes: 0, target_count: 2, methods: methodsFor(0), weekdays: ALL_DAYS },
  ]);
  let newGoal = $state('');
  let newMinutes = $state(30);
  let essentials = $state('windows:pfn:5319275A.WhatsAppDesktop_cv1g1gvanyjgm');
  let focusApps = $state('windows:exe:Microsoft Corporation|winword.exe\nwindows:exe:Google LLC|chrome.exe');
  let web = $state('udemy.com\ncoursera.org');
  let relay = $state('');
  let qr = $state('');
  let code = $state<string | null>(null);
  let busy = $state(false);

  const lines = (s: string) => s.split('\n').map((l) => l.trim()).filter(Boolean);
  const toggleDay = (i: number) => (freeDays ^= 1 << i);

  function addGoal() {
    const t = newGoal.trim();
    if (!t) return;
    goals.push({
      id: slug(t, goals.map((g) => g.id)), title: t, kind: 'key', target_minutes: newMinutes,
      target_count: 1, methods: methodsFor(newMinutes), weekdays: ALL_DAYS,
    });
    newGoal = '';
  }

  async function create() {
    busy = true;
    try {
      const config: Config = {
        schedule: { wake: { minutes: toMinutes(wake) }, bedtime: { minutes: toMinutes(bedtime) } },
        permits_per_day: Math.min(10, Math.max(0, permits)),
        permit_minutes: Math.min(60, Math.max(1, permitMinutes)),
        permit_cooldown_minutes: 60,
        emergency_minutes: 15,
        emergency_penalty: 1,
        habit_penalty: true,
        goals,
        free_days: freeDays,
        essential_apps: lines(essentials),
        focus_apps: lines(focusApps),
        web_allowlist: lines(web).map((d) => d.toLowerCase()),
      };
      await call({ cmd: 'create_profile', label: label.trim() || 'PC', config });
      if (relay.trim()) await call({ cmd: 'connect_relay', url: relay.trim() });
      ondone();
    } catch (e) {
      onerror(String(e));
    } finally {
      busy = false;
    }
  }

  async function join() {
    busy = true;
    try {
      code = await text({ cmd: 'pairing_join', qr: qr.trim(), label: label.trim() || 'PC' });
      // El otro dispositivo confirma y nos envía el historial; la pantalla avanza sola.
      for (let i = 0; i < 100; i++) {
        await new Promise((r) => setTimeout(r, 3000));
        await call({ cmd: 'sync' }).catch(() => {});
        const r = await call({ cmd: 'status' });
        if (r.type === 'status' && r.agent.core) return ondone();
      }
    } catch (e) {
      onerror(String(e));
    } finally {
      busy = false;
    }
  }
</script>

<main class="onb stack">
  <header class="stack" style="gap:6px">
    <h1>Bienvenido a ProcastinaciOFF</h1>
    <p class="muted">Tus metas primero; el resto, después. Todo se procesa en tus dispositivos y viaja cifrado.</p>
  </header>

  <div class="row" role="tablist" aria-label="Cómo empezar">
    <button role="tab" aria-selected={mode === 'new'} class={mode === 'new' ? 'btn-tonal' : 'btn-text'} onclick={() => (mode = 'new')}>Crear mi perfil</button>
    <button role="tab" aria-selected={mode === 'join'} class={mode === 'join' ? 'btn-tonal' : 'btn-text'} onclick={() => (mode = 'join')}>Unirme a un perfil</button>
  </div>

  <label><span>Nombre de este equipo</span><input bind:value={label} maxlength="64" /></label>

  {#if mode === 'new'}
    <section class="card stack" aria-labelledby="h-sleep">
      <h2 id="h-sleep">Horario</h2>
      <div class="row">
        <label><span>Despertar</span><input type="time" bind:value={wake} /></label>
        <label><span>Dormir</span><input type="time" bind:value={bedtime} /></label>
      </div>
      <fieldset class="days">
        <legend>Días libres <small class="muted">(sin metas ni bloqueo, y sin hora de dormir la noche anterior)</small></legend>
        {#each DAY_LABELS as d, i}
          <label class="day">
            <input type="checkbox" checked={(freeDays & (1 << i)) !== 0} onchange={() => toggleDay(i)} aria-label={DAY_NAMES[i]} />
            <span>{d}</span>
          </label>
        {/each}
      </fieldset>
      <div class="row">
        <label><span>Permisos al día</span><input type="number" min="0" max="10" bind:value={permits} /></label>
        <label><span>Minutos por permiso</span><input type="number" min="1" max="60" bind:value={permitMinutes} /></label>
      </div>
      <p class="muted small">Relajar estas reglas después solo entra en vigor al día siguiente; endurecerlas, al momento.</p>
    </section>

    <section class="card stack" aria-labelledby="h-goals">
      <h2 id="h-goals">Metas fijas</h2>
      <p class="muted small">Aparecen cada mañana en el plan del día. Allí podrás añadir más.</p>
      <ul class="list">
        {#each goals as g, i (g.id)}
          <li class="row">
            <strong>{g.title}</strong>
            <span class="chip {g.kind === 'key' ? '' : 'neutral'}">{g.kind === 'key' ? 'Desbloquea' : 'Hábito'}</span>
            <span class="muted">{g.target_minutes ? duration(g.target_minutes) : `${g.target_count} veces`}</span>
            <button class="btn-text" onclick={() => goals.splice(i, 1)}>Quitar</button>
          </li>
        {/each}
      </ul>
      <div class="row">
        <label class="grow"><span>Nueva meta que desbloquea</span><input bind:value={newGoal} maxlength="80" /></label>
        <label><span>Minutos</span><input type="number" min="0" max="600" step="5" bind:value={newMinutes} /></label>
        <button class="btn-secondary" disabled={!newGoal.trim()} onclick={addGoal}>Añadir</button>
      </div>
    </section>

    <details class="card">
      <summary>Apps y sitios permitidos</summary>
      <div class="stack" style="margin-top:12px">
        <label><span>Apps esenciales (siempre, salvo al dormir), una por línea</span><textarea rows="2" bind:value={essentials}></textarea></label>
        <label><span>Apps de enfoque, una por línea</span><textarea rows="2" bind:value={focusApps}></textarea></label>
        <label><span>Sitios permitidos durante el enfoque</span><textarea rows="2" bind:value={web}></textarea></label>
        <label><span>Servidor de sincronización (opcional)</span><input bind:value={relay} placeholder="https://…" /></label>
      </div>
    </details>

    <button class="btn-primary" disabled={busy || !goals.some((g) => g.kind === 'key')} onclick={create}>Crear perfil</button>
  {:else if !code}
    <section class="card stack">
      <p>En el otro dispositivo abre «Vincular dispositivo» y copia el código. Pégalo aquí:</p>
      <textarea rows="4" bind:value={qr} aria-label="Código de vinculación"></textarea>
      <button class="btn-primary" disabled={busy || !qr.trim()} onclick={join}>Unirme</button>
    </section>
  {:else}
    <section class="card stack" aria-live="polite">
      <p>Comprueba que el otro dispositivo muestra:</p>
      <p class="code num">{code}</p>
      <p class="muted">Confirma allí. Esta pantalla avanzará sola.</p>
    </section>
  {/if}
</main>

<style>
  .onb { max-width: 760px; margin: 0 auto; padding: 32px 16px 48px; }
  .days { border: 0; padding: 0; margin: 0; display: flex; gap: 8px; flex-wrap: wrap; }
  .days legend { margin-bottom: 8px; font-weight: 600; color: var(--muted); font-size: 14px; }
  .day { display: inline-flex; flex-direction: row; align-items: center; gap: 4px; color: var(--text); }
  .day input { min-height: auto; width: 18px; height: 18px; }
  .list { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
  .grow { flex: 1 1 220px; }
  .small { font-size: 14px; }
  .code { font-size: 48px; font-weight: 700; letter-spacing: 0.12em; }
  summary { cursor: pointer; font-weight: 600; }
</style>
