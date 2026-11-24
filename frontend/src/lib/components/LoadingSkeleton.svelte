<script lang="ts">
  interface Props {
    variant?: 'dashboard' | 'dss' | 'conf' | 'article';
    message?: string;
  }

  let {
    variant = 'dashboard',
    message = 'Завантаження актуального стану Web-агента та ІСППР…',
  }: Props = $props();
</script>

<div class="loading-shell" role="status" aria-live="polite" aria-busy="true">
  <div class="loading-banner">
    <span class="spinner" aria-hidden="true"></span>
    <div>
      <div class="loading-title">{message}</div>
      <div class="loading-sub">
        Отримання даних із JSON API Rust-сервера та підключення live-каналу WebSocket…
      </div>
    </div>
  </div>

  {#if variant === 'dashboard'}
    <section class="panel sk-hero">
      <div class="sk-hero-top">
        <div class="sk-stack">
          <div class="sk-line sk-pill"></div>
          <div class="sk-line sk-h1"></div>
          <div class="sk-line sk-body"></div>
          <div class="sk-line sk-body short"></div>
        </div>
        <div class="sk-kpi-grid">
          <div class="sk-box"></div>
          <div class="sk-box"></div>
          <div class="sk-box"></div>
          <div class="sk-box"></div>
        </div>
      </div>
    </section>

    <div class="sk-split">
      <section class="panel">
        <div class="sk-line sk-h2"></div>
        <div class="sk-card"></div>
        <div class="sk-card"></div>
      </section>
      <section class="panel">
        <div class="sk-line sk-h2"></div>
        <div class="sk-kpi-grid">
          <div class="sk-box"></div>
          <div class="sk-box"></div>
        </div>
      </section>
    </div>

    <section class="panel">
      <div class="sk-line sk-h2"></div>
      <div class="sk-row"></div>
      <div class="sk-row"></div>
      <div class="sk-row"></div>
    </section>
  {:else if variant === 'dss'}
    <section class="panel sk-hero">
      <div class="sk-hero-top">
        <div class="sk-stack">
          <div class="sk-line sk-pill"></div>
          <div class="sk-line sk-h1"></div>
          <div class="sk-line sk-body"></div>
        </div>
        <div class="sk-kpi-grid">
          <div class="sk-box"></div>
          <div class="sk-box"></div>
          <div class="sk-box"></div>
          <div class="sk-box"></div>
        </div>
      </div>
    </section>

    <section class="panel">
      <div class="sk-line sk-h2"></div>
      <div class="sk-card tall"></div>
      <div class="sk-card tall"></div>
    </section>
  {:else if variant === 'conf'}
    <div class="sk-three">
      <section class="panel">
        <div class="sk-line sk-h2"></div>
        <div class="sk-row"></div>
        <div class="sk-row"></div>
        <div class="sk-row"></div>
      </section>
      <section class="panel">
        <div class="sk-line sk-h2"></div>
        <div class="sk-row"></div>
        <div class="sk-row"></div>
      </section>
      <section class="panel">
        <div class="sk-line sk-h2"></div>
        <div class="sk-row"></div>
        <div class="sk-row"></div>
        <div class="sk-row"></div>
      </section>
    </div>

    <section class="panel">
      <div class="sk-line sk-h2"></div>
      <div class="sk-row"></div>
      <div class="sk-row"></div>
    </section>
  {:else}
    <section class="panel">
      <div class="sk-stack">
        <div class="sk-line sk-pill"></div>
        <div class="sk-line sk-h1"></div>
        <div class="sk-card tall"></div>
        <div class="sk-line sk-body"></div>
        <div class="sk-line sk-body short"></div>
      </div>
    </section>
  {/if}
</div>

<style>
  .loading-shell {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .loading-banner {
    display: flex;
    align-items: center;
    gap: 0.85rem;
    padding: 0.85rem 1.15rem;
    margin-bottom: 1.1rem;
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    border: 1px solid var(--accent-border);
    color: var(--info-fg);
  }

  .spinner {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2.5px solid rgba(37, 99, 235, 0.22);
    border-top-color: var(--accent);
    animation: sk-spin 0.75s linear infinite;
    flex-shrink: 0;
  }

  .loading-title {
    font-size: 0.92rem;
    font-weight: 700;
    color: var(--text);
  }

  .loading-sub {
    font-size: 0.8rem;
    color: var(--text-secondary);
  }

  .sk-hero-top {
    display: grid;
    grid-template-columns: minmax(0, 1.15fr) minmax(0, 1.35fr);
    gap: 1.25rem;
    align-items: start;
  }

  .sk-stack {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }

  .sk-kpi-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.7rem;
  }

  .sk-split {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1.25rem;
  }

  .sk-three {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 1.15rem;
  }

  .sk-line,
  .sk-box,
  .sk-card,
  .sk-row {
    border-radius: var(--radius-sm);
    background: linear-gradient(90deg, #e2e8f0 25%, #f1f5f9 50%, #e2e8f0 75%);
    background-size: 200% 100%;
    animation: sk-shimmer 1.35s ease-in-out infinite;
  }

  .sk-pill {
    width: 170px;
    height: 22px;
    border-radius: 999px;
  }

  .sk-h1 {
    width: 72%;
    height: 30px;
  }

  .sk-h2 {
    width: 240px;
    max-width: 65%;
    height: 22px;
    margin-bottom: 0.9rem;
  }

  .sk-body {
    width: 92%;
    height: 15px;
  }

  .sk-body.short {
    width: 64%;
  }

  .sk-box {
    height: 82px;
    border-radius: var(--radius-md);
  }

  .sk-card {
    height: 92px;
    border-radius: var(--radius-md);
    margin-bottom: 0.75rem;
  }

  .sk-card.tall {
    height: 132px;
  }

  .sk-card:last-child {
    margin-bottom: 0;
  }

  .sk-row {
    height: 44px;
    margin-bottom: 0.6rem;
  }

  .sk-row:last-child {
    margin-bottom: 0;
  }

  @keyframes sk-spin {
    to {
      transform: rotate(360deg);
    }
  }

  @keyframes sk-shimmer {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }

  @media (max-width: 960px) {
    .sk-hero-top,
    .sk-split,
    .sk-three {
      grid-template-columns: 1fr;
    }
  }
</style>
