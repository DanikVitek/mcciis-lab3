<script lang="ts">
  import '#lib/styles/tokens.css';
  import AppHeader from '#lib/components/AppHeader.svelte';
  import { setNotify, type NoticeKind } from '#lib/notify-context.ts';
  import type { LayoutProps } from './$types';

  let { data, children }: LayoutProps = $props();

  let toast = $state<{ kind: NoticeKind; text: string } | null>(null);
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  function notify(kind: NoticeKind, text: string) {
    toast = { kind, text };
    if (hideTimer) clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      toast = null;
    }, 5500);
  }

  setNotify(notify);
</script>

<div class="app-shell">
  <AppHeader snapshot={data.uiState.snapshot} onNotify={notify} />

  {#if toast}
    <div class="toast-container">
      <div class={['toast', `toast-${toast.kind}`]} role="status" aria-live="polite">
        <span>{toast.text}</span>
        <button
          type="button"
          class="toast-close"
          aria-label="Закрити повідомлення"
          onclick={() => (toast = null)}
        >
          ×
        </button>
      </div>
    </div>
  {/if}

  <main class="page-main">
    {@render children()}
  </main>

  <footer class="site-footer">
    <div class="footer-inner">
      <span>Інтелектуальний Web-агент та продукційна ІСППР • Лабораторна робота № 3 (СКСІІС)</span>
      <span class="mono">SvelteKit Static SPA + Svelte 5 • Rust (Axum)</span>
    </div>
  </footer>
</div>

<style>
  .app-shell {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }

  .toast-container {
    max-width: 1320px;
    width: 100%;
    margin: 0.9rem auto 0;
    padding: 0 1.5rem;
  }

  .toast {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
    padding: 0.7rem 1rem;
    border-radius: var(--radius-md);
    font-size: 0.88rem;
    font-weight: 600;
    border: 1px solid var(--border);
  }

  .toast-ok {
    background: var(--ok-bg);
    color: var(--ok-fg);
    border-color: var(--ok-border);
  }

  .toast-info {
    background: var(--info-bg);
    color: var(--info-fg);
    border-color: var(--info-border);
  }

  .toast-error {
    background: var(--critical-bg);
    color: var(--critical-fg);
    border-color: var(--critical-border);
  }

  .toast-close {
    background: transparent;
    border: none;
    color: inherit;
    font-size: 1.2rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.25rem;
  }

  .page-main {
    flex: 1 0 auto;
    max-width: 1320px;
    width: 100%;
    margin: 1.4rem auto 2rem;
    padding: 0 1.5rem;
  }

  .site-footer {
    margin-top: auto;
    border-top: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted);
    font-size: 0.82rem;
  }

  .footer-inner {
    max-width: 1320px;
    margin: 0 auto;
    padding: 0.9rem 1.5rem;
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
  }
</style>
