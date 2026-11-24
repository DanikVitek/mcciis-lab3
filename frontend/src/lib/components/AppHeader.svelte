<script lang="ts">
  import { invalidateAll } from '$app/navigation';
  import { page } from '$app/state';
  import { triggerPoll } from '#lib/api.ts';
  import {
    describeOperationalMode,
    extractOperationalMode,
    isSiteChangedVisible,
  } from '#lib/format.ts';
  import type { AgentSnapshot } from '#lib/schemas.ts';
    import { resolve } from '$app/paths';

  interface Props {
    snapshot: AgentSnapshot;
    onNotify?: (kind: 'ok' | 'info' | 'error', text: string) => void;
  }

  let { snapshot, onNotify }: Props = $props();

  let polling = $state(false);

  let modeInfo = $derived(describeOperationalMode(extractOperationalMode(snapshot)));
  let changedCount = $derived(snapshot.sites.filter(isSiteChangedVisible).length);
  let decisionsCount = $derived(snapshot.inference.decisions.length);

  let pathname = $derived(page.url.pathname);
  let isIndexActive = $derived(pathname === '/' || pathname.startsWith('/article/'));
  let isDssActive = $derived(pathname === '/dss');
  let isConfActive = $derived(pathname === '/conf');

  async function handlePoll() {
    polling = true;
    try {
      await triggerPoll();
      await invalidateAll();
      onNotify?.(
        'ok',
        'Позачергове опитування Web-джерел запущено. Стан оновиться автоматично.',
      );
      setTimeout(() => {
        void invalidateAll();
      }, 1800);
    } catch (err) {
      const msg =
        err instanceof Error ? err.message : 'Не вдалося запустити позачергове опитування.';
      onNotify?.('error', msg);
    } finally {
      polling = false;
    }
  }
</script>

<header class="site-header">
  <div class="header-inner">
    <a class="brand" href={resolve("/")}>
      <span class="brand-title">
        <span class="brand-dot" aria-hidden="true"></span>
        Web-агент та ІСППР
      </span>
      <span class="brand-sub">Моніторинг Web-джерел і підтримка рішень</span>
    </a>

    <nav class="header-nav" aria-label="Основна навігація">
      <a href={resolve("/")} class={['nav-link', isIndexActive && 'active']}>
        Огляд і моніторинг
        {#if changedCount > 0}
          <span class="nav-count num" title="Непереглянуті зміни сайтів">{changedCount}</span>
        {/if}
      </a>

      <a href={resolve("/dss")} class={['nav-link', isDssActive && 'active']}>
        Рішення ІСППР
        {#if decisionsCount > 0}
          <span class="nav-count num" title="Активні рішення ІСППР">{decisionsCount}</span>
        {/if}
      </a>

      <a href={resolve("/conf")} class={['nav-link', isConfActive && 'active']}>
        Конфігурація
      </a>

      <a class="nav-link api-link" href="/api/state" target="_blank" rel="noopener">
        JSON API ↗
      </a>
    </nav>

    <div class="header-actions">
      <a
        href={resolve("/dss")}
        class={['mode-chip', `tone-${modeInfo.tone}`]}
        title={`Операційний режим ІСППР (${modeInfo.code})`}
      >
        <span class="dot" aria-hidden="true"></span>
        <span>{modeInfo.title}</span>
      </a>

      <button type="button" class="btn poll-btn" disabled={polling} onclick={handlePoll}>
        {#if polling}
          Оновлення…
        {:else}
          ↻ Оновити зараз
        {/if}
      </button>
    </div>
  </div>
</header>

<style>
  .site-header {
    background: linear-gradient(180deg, #0f172a 0%, #16213a 100%);
    color: #f8fafc;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .header-inner {
    max-width: 1320px;
    margin: 0 auto;
    padding: 0.85rem 1.5rem;
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.85rem;
  }

  .brand {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    text-decoration: none;
    color: inherit;
  }

  .brand:hover {
    color: #ffffff;
  }

  .brand-title {
    font-size: 1.06rem;
    font-weight: 700;
    letter-spacing: -0.01em;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .brand-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #38bdf8;
    box-shadow: 0 0 0 3px rgba(56, 189, 248, 0.25);
    flex-shrink: 0;
  }

  .brand-sub {
    font-size: 0.76rem;
    color: #94a3b8;
  }

  .header-nav {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.3rem;
  }

  .nav-link {
    display: inline-flex;
    align-items: center;
    gap: 0.38rem;
    color: #cbd5e1;
    text-decoration: none;
    padding: 0.42rem 0.75rem;
    border-radius: var(--radius-sm);
    font-size: 0.86rem;
    font-weight: 500;
    transition: background 0.15s, color 0.15s;
  }

  .nav-link:hover {
    background: rgba(255, 255, 255, 0.09);
    color: #ffffff;
  }

  .nav-link.active {
    background: var(--accent);
    color: #ffffff;
  }

  .api-link {
    color: #94a3b8;
    font-size: 0.81rem;
  }

  .nav-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.25rem;
    height: 1.25rem;
    padding: 0 0.35rem;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.16);
    font-size: 0.72rem;
    font-weight: 700;
  }

  .nav-link.active .nav-count {
    background: rgba(255, 255, 255, 0.25);
  }

  .header-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.55rem;
  }

  .mode-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.34rem 0.72rem;
    border-radius: 999px;
    font-size: 0.78rem;
    font-weight: 600;
    background: rgba(255, 255, 255, 0.08);
    color: #e2e8f0;
    border: 1px solid rgba(255, 255, 255, 0.14);
    text-decoration: none;
    white-space: nowrap;
  }

  .mode-chip:hover {
    background: rgba(255, 255, 255, 0.14);
    color: #ffffff;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .tone-critical .dot {
    background: #ef4444;
    box-shadow: 0 0 0 3px rgba(239, 68, 68, 0.32);
  }

  .tone-high .dot {
    background: #f97316;
    box-shadow: 0 0 0 3px rgba(249, 115, 22, 0.32);
  }

  .tone-medium .dot {
    background: #eab308;
  }

  .tone-ok .dot {
    background: #10b981;
  }

  .tone-info .dot {
    background: #38bdf8;
  }

  .poll-btn {
    white-space: nowrap;
  }
</style>
