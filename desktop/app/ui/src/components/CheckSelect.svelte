<script lang="ts">
  // How a goal is checked. While the person has not picked one, it follows the goal's name (the core's keyword
  // catalog, see goals.rs) and says so. `suggest` is false when editing an existing goal.
  import type { Check } from '../lib/agent';
  import { suggestChecks } from '../lib/native';
  import { t } from '../lib/i18n';

  let {
    title, check, options, suggest, onpick,
  }: { title: string; check: Check; options: Check[]; suggest: boolean; onpick: (c: Check) => void } = $props();

  let picked = $state(false);
  let suggested = $state(false);

  $effect(() => {
    const name = title.trim();
    if (!name) {
      picked = false;
      suggested = false;
    }
    if (!suggest || picked || !name) return;
    // Typing: only the name it stops at
    const id = setTimeout(async () => {
      const first = (await suggestChecks(name)).find((c) => options.includes(c));
      if (first && !picked && title.trim() === name) {
        suggested = true;
        onpick(first);
      }
    }, 500);
    return () => clearTimeout(id);
  });
</script>

<label><span>{t('plan.how_checked')}</span>
  <select value={check} onchange={(e) => { picked = true; onpick(e.currentTarget.value as Check); }}>
    {#each options as value (value)}<option {value}>{t(`check.${value}`)}</option>{/each}
  </select>
</label>
{#if suggest && suggested && !picked}<p class="muted small hint">{t('goals.suggested_by_name')}</p>{/if}
{#if check === 'health'}<p class="muted small hint">{t('goals.health_note')}</p>{/if}

<style>
  .small { font-size: 14px; }
  .hint { flex-basis: 100%; margin: 0; }
</style>
