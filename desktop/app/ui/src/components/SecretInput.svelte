<script lang="ts">
  // A PIN, password or pattern as typed. Pattern: cells of a 3×3 grid in order, sent as "0-4-8-5".
  import type { LocalFactor } from '../lib/agent';
  import { t } from '../lib/i18n';

  let { factor, value = $bindable('') }: { factor: LocalFactor; value?: string } = $props();
  const cells = $derived(value ? value.split('-').map(Number) : []);
</script>

{#if factor === 'pattern'}
  <div class="grid" role="group" aria-label={t('settings.factor_pattern')}>
    {#each [0, 1, 2, 3, 4, 5, 6, 7, 8] as i (i)}
      <button
        class="cell"
        aria-pressed={cells.includes(i)}
        onclick={() => {
          if (!cells.includes(i)) value = [...cells, i].join('-');
        }}>{cells.includes(i) ? cells.indexOf(i) + 1 : ''}</button
      >
    {/each}
  </div>
  <button class="btn-text clear" onclick={() => (value = '')}>{t('settings.pattern_clear')}</button>
{:else}
  <label>
    <span>{t(`settings.factor_${factor}`)}</span>
    <input type="password" inputmode={factor === 'pin' ? 'numeric' : 'text'} autocomplete="off" bind:value />
  </label>
{/if}

<style>
  .grid { display: grid; grid-template-columns: repeat(3, 56px); gap: 12px; }
  .cell { width: 56px; height: 56px; border-radius: 50%; border: 1.5px solid var(--border); background: var(--surface); font-weight: 700; }
  .cell[aria-pressed='true'] { background: var(--accent-soft); border-color: var(--accent); color: var(--accent); }
  .clear { justify-self: start; }
</style>
