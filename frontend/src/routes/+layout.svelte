<script lang="ts">
  import '#lib/styles/tokens.css';
  import AppHeader from '#lib/components/AppHeader.svelte';
  import { setNotify, type NoticeKind } from '#lib/notify-context.ts';
  import { setUiState, UiStateStore } from '#lib/ui-state.svelte.ts';
  import type { LayoutProps } from './$types';

  let { children }: LayoutProps = $props();

  let nextToastId = 0;
  let toast = $state<{ id: number; kind: NoticeKind; text: string } | null>(null);
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  function dismissToast() {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = undefined;
    }
    toast = null;
  }

  function notify(kind: NoticeKind, text: string) {
    toast = { id: ++nextToastId, kind, text };
    if (hideTimer) clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      toast = null;
      hideTimer = undefined;
    }, 5500);
  }

  const store = new UiStateStore(notify);

  setNotify(notify);
  setUiState(store);
</script>

<div class="app-shell">
  <AppHeader
    snapshot={store.payload?.snapshot ?? null}
    wsStatus={store.wsStatus}
    pollInProgress={store.isPollRunning}
    isSyncing={store.isSyncing}
    onPollApplied={(next) => store.applyPayload(next)}
    onNotify={notify}
  />

  {#if store.isPollRunning}
    <div class="live-bar-wrap">
      <div class="live-bar" role="status" aria-live="polite">
        <span class="live-spinner" aria-hidden="true"></span>
        <span>
          Web-агент виконує цикл опитування зовнішніх джерел… Інтерфейс оновиться автоматично після
          завершення ітерації.
        </span>
      </div>
    </div>
  {/if}

  <main class="page-main">
    {@render children()}
  </main>

  <footer class="site-footer">
    <div class="footer-inner">
      <span>Інтелектуальний Web-агент та продукційна ІСППР • Лабораторна робота № 3 (СКСІІС)</span>
      <span class="mono">SvelteKit Static SPA + Svelte 5 • Rust (Axum + WebSocket)</span>
    </div>
  </footer>

  {#if toast}
    <div class="toast-viewport" aria-live="polite" aria-atomic="true">
      {#key toast.id}
        <div class={['toast', `toast-${toast.kind}`]} role="status">
          <span class="toast-text">{toast.text}</span>
          <button
            type="button"
            class="toast-close"
            aria-label="Закрити повідомлення"
            onclick={dismissToast}
          >
            ×
          </button>
        </div>
      {/key}
    </div>
  {/if}
</div>

<style>
  .app-shell {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }

  .live-bar-wrap {
    max-width: 1320px;
    width: 100%;
    margin: 0.9rem auto 0;
    padding: 0 1.5rem;
  }

  .live-bar {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    padding: 0.6rem 0.95rem;
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    border: 1px solid var(--accent-border);
    color: var(--info-fg);
    font-size: 0.85rem;
    font-weight: 600;
  }

  .live-spinner {
    width: 15px;
    height: 15px;
    border-radius: 50%;
    border: 2px solid rgba(37, 99, 235, 0.25);
    border-top-color: var(--accent);
    animation: live-spin 0.75s linear infinite;
    flex-shrink: 0;
  }

  @keyframes live-spin {
    to {
      transform: rotate(360deg);
    }
  }

  .toast-viewport {
    position: fixed;
    right: 1.25rem;
    bottom: 1.25rem;
    z-index: 60;
    width: min(440px, calc(100vw - 2rem));
    pointer-events: none;
  }

  .toast {
    pointer-events: auto;
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.75rem;
    padding: 0.8rem 1rem;
    border-radius: var(--radius-md);
    font-size: 0.86rem;
    font-weight: 600;
    line-height: 1.45;
    border: 1px solid var(--border);
    box-shadow:
      0 14px 32px rgba(15, 23, 42, 0.16),
      0 3px 8px rgba(15, 23, 42, 0.08);
    animation: toast-in 0.18s ease-out;
  }

  .toast-text {
    min-width: 0;
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
  }

  @media (max-width: 640px) {
    .toast-viewport {
      right: 1rem;
      bottom: 1rem;
      left: 1rem;
      width: auto;
    }
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
    flex-shrink: 0;
    opacity: 0.8;
    transition: opacity 0.15s;
  }

  .toast-close:hover {
    opacity: 1;
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
