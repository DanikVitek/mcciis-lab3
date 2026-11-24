<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    text: string;
    title?: string;
    position?: 'top' | 'bottom';
    align?: 'center' | 'start' | 'end';
    dark?: boolean;
    label?: string;
    children?: Snippet;
  }

  let {
    text,
    title,
    position = 'top',
    align = 'center',
    dark = false,
    label = 'Пояснення',
    children,
  }: Props = $props();

  const tooltipId = $props.id();
</script>

<span class={['tt-wrap', dark && 'tt-dark']}>
  {#if children}
    <span class="tt-trigger-slot" aria-describedby={tooltipId}>
      {@render children()}
    </span>
  {:else}
    <button
      type="button"
      class="tt-icon"
      aria-label={title ? `${label}: ${title}` : label}
      aria-describedby={tooltipId}
    >
      ?
    </button>
  {/if}

  <span
    id={tooltipId}
    role="tooltip"
    class={['tt-bubble', `pos-${position}`, `align-${align}`]}
  >
    {#if title}
      <span class="tt-title">{title}</span>
    {/if}
    <span class="tt-text">{text}</span>
  </span>
</span>

<style>
  .tt-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
    vertical-align: middle;
  }

  .tt-trigger-slot {
    display: inline-flex;
    align-items: center;
  }

  .tt-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.12rem;
    height: 1.12rem;
    padding: 0;
    border-radius: 50%;
    border: 1px solid var(--border-strong);
    background: var(--surface-subtle);
    color: var(--muted);
    font-size: 0.7rem;
    font-weight: 700;
    line-height: 1;
    font-family: inherit;
    cursor: help;
    flex-shrink: 0;
    transition:
      background 0.15s,
      color 0.15s,
      border-color 0.15s;
  }

  .tt-icon:hover,
  .tt-icon:focus-visible {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent-border);
    outline: none;
  }

  .tt-dark .tt-icon {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.24);
    color: #cbd5e1;
  }

  .tt-dark .tt-icon:hover,
  .tt-dark .tt-icon:focus-visible {
    background: rgba(56, 189, 248, 0.22);
    border-color: rgba(56, 189, 248, 0.55);
    color: #ffffff;
  }

  .tt-bubble {
    position: absolute;
    z-index: 80;
    width: max-content;
    max-width: min(290px, 82vw);
    padding: 0.55rem 0.75rem;
    border-radius: var(--radius-sm);
    background: #0f172a;
    color: #f8fafc;
    border: 1px solid rgba(148, 163, 184, 0.28);
    box-shadow: 0 10px 24px rgba(15, 23, 42, 0.24);
    font-size: 0.78rem;
    font-weight: 400;
    line-height: 1.45;
    text-transform: none;
    letter-spacing: normal;
    white-space: normal;
    text-align: left;
    pointer-events: none;
    opacity: 0;
    visibility: hidden;
    transition:
      opacity 0.14s ease,
      transform 0.14s ease,
      visibility 0.14s ease;
  }

  .tt-title {
    display: block;
    font-weight: 700;
    color: #ffffff;
    margin-bottom: 0.2rem;
    font-size: 0.79rem;
  }

  .tt-text {
    display: block;
    color: #e2e8f0;
  }

  .pos-top {
    bottom: calc(100% + 8px);
    transform: translateY(3px);
  }

  .pos-bottom {
    top: calc(100% + 8px);
    transform: translateY(-3px);
  }

  .align-center {
    left: 50%;
    transform: translateX(-50%) translateY(3px);
  }

  .pos-bottom.align-center {
    transform: translateX(-50%) translateY(-3px);
  }

  .align-start {
    left: 0;
  }

  .align-end {
    right: 0;
  }

  .tt-wrap:hover .tt-bubble,
  .tt-wrap:focus-within .tt-bubble {
    opacity: 1;
    visibility: visible;
    transform: translateY(0);
  }

  .tt-wrap:hover .tt-bubble.align-center,
  .tt-wrap:focus-within .tt-bubble.align-center {
    transform: translateX(-50%) translateY(0);
  }
</style>
