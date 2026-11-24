<script lang="ts">
  import type { Tone } from '#lib/schemas.ts';

  interface Props {
    value: number;
    max?: number;
    tone?: 'accent' | Tone;
    label?: string;
    width?: string;
  }

  let { value, max = 1, tone = 'accent', label, width = '64px' }: Props = $props();

  let pct = $derived(
    max > 0 ? Math.max(0, Math.min(100, Math.round((value / max) * 100))) : 0,
  );
</script>

<span class="meter-wrap" title={label ? `${label}: ${pct}%` : `${pct}%`}>
  <span class="track" style:--meter-width={width}>
    <span class={['fill', `tone-${tone}`]} style:--pct={`${pct}%`}></span>
  </span>
  {#if label}
    <span class="meter-label mono num">{label}</span>
  {/if}
</span>

<style>
  .meter-wrap {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
  }

  .track {
    display: inline-block;
    width: var(--meter-width, 64px);
    height: 6px;
    border-radius: 999px;
    background: #e2e8f0;
    overflow: hidden;
    flex-shrink: 0;
  }

  .fill {
    display: block;
    height: 100%;
    width: var(--pct, 0%);
    border-radius: 999px;
    background: var(--accent);
    transition: width 0.2s ease;
  }

  .tone-critical {
    background: var(--critical-solid);
  }

  .tone-high {
    background: var(--high-solid);
  }

  .tone-medium {
    background: var(--medium-solid);
  }

  .tone-info {
    background: var(--info-solid);
  }

  .tone-ok {
    background: var(--ok-solid);
  }

  .meter-label {
    font-size: 0.8rem;
    color: var(--text-secondary);
  }
</style>
