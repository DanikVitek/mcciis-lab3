<script lang="ts">
  import { invalidateAll } from "$app/navigation";
  import {
    ackSite,
    enableFeed,
    enableNbu,
    enableSite,
    unackSite,
  } from "#lib/api.ts";
  import CfMeter from "#lib/components/CfMeter.svelte";
  import {
    countDisabledSources,
    describeOperationalMode,
    errorKindLabelUk,
    extractOperationalMode,
    formatBytes,
    formatDateTimeUk,
    formatSizeDelta,
    groupLabelUk,
    isHealthThrottled,
    isSiteChangedVisible,
    severityLabelUk,
    shortPreview,
  } from "#lib/format.ts";
  import { getNotify } from "#lib/notify-context.ts";
  import type {
    HttpSiteState,
    NewsArticle,
    SourceHealth,
  } from "#lib/schemas.ts";
  import type { PageProps } from "./$types";
  import { resolve } from "$app/paths";

  let { data }: PageProps = $props();
  const notify = getNotify();

  let snapshot = $derived(data.uiState.snapshot);

  type SiteFilter = "all" | "changed" | "ack";
  type SortMode = "score" | "date";

  let siteFilter = $state<SiteFilter>("all");
  let searchQuery = $state("");
  let selectedGroup = $state<string>("all");
  let onlyCve = $state(false);
  let sortMode = $state<SortMode>("score");
  let visibleArticlesLimit = $state(10);
  let busyKey = $state<string | null>(null);

  let modeInfo = $derived(
    describeOperationalMode(extractOperationalMode(snapshot)),
  );
  let changedSites = $derived(snapshot.sites.filter(isSiteChangedVisible));
  let disabledCount = $derived(countDisabledSources(snapshot));

  let faultSites = $derived(
    snapshot.sites.filter(
      (s) => s.health.disabled || isHealthThrottled(s.health),
    ),
  );
  let faultFeeds = $derived(
    snapshot.feeds.filter(
      (f) => f.health.disabled || isHealthThrottled(f.health),
    ),
  );
  let nbuHasFault = $derived(
    snapshot.nbu_health.disabled || isHealthThrottled(snapshot.nbu_health),
  );
  let hasAnyFault = $derived(
    faultSites.length > 0 || faultFeeds.length > 0 || nbuHasFault,
  );

  let filteredSites = $derived.by(() => {
    const list = snapshot.sites.filter((site) => {
      if (siteFilter === "changed") return isSiteChangedVisible(site);
      if (siteFilter === "ack") return site.acknowledged;
      return true;
    });
    return [...list].sort((a, b) => {
      const aChanged = isSiteChangedVisible(a) ? 0 : 1;
      const bChanged = isSiteChangedVisible(b) ? 0 : 1;
      if (aChanged !== bChanged) return aChanged - bChanged;
      return a.name.localeCompare(b.name, "uk");
    });
  });

  let articleGroups = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const art of snapshot.articles) {
      counts.set(art.group, (counts.get(art.group) ?? 0) + 1);
    }
    return Array.from(counts.entries()).sort((a, b) => b[1] - a[1]);
  });

  let totalCveArticles = $derived(
    snapshot.articles.filter((a) => a.cves.length > 0).length,
  );

  let filteredArticles = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    const list = snapshot.articles.filter((art) => {
      if (selectedGroup !== "all" && art.group !== selectedGroup) return false;
      if (onlyCve && art.cves.length === 0) return false;
      if (q) {
        const hay = [
          art.title,
          art.feed_name,
          art.summary,
          ...art.cves,
          ...art.matched_keywords.map((k) => k.word),
        ]
          .join(" ")
          .toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });

    if (sortMode === "date") {
      return [...list].sort((a, b) => {
        const ta = a.published ? Date.parse(a.published) : 0;
        const tb = b.published ? Date.parse(b.published) : 0;
        if (tb !== ta) return tb - ta;
        return b.relevance_score - a.relevance_score;
      });
    }
    return list;
  });

  let shownArticles = $derived(filteredArticles.slice(0, visibleArticlesLimit));
  let maxArticleScore = $derived(
    snapshot.articles.reduce((max, a) => Math.max(max, a.relevance_score), 1),
  );
  let topDecisions = $derived(snapshot.inference.decisions.slice(0, 3));

  function siteStatusBadge(site: HttpSiteState): {
    label: string;
    cls: string;
  } {
    if (site.health.disabled) return { label: "Вимкнено", cls: "sev-critical" };
    if (isHealthThrottled(site.health))
      return { label: "Пауза (backoff)", cls: "sev-high" };
    if (isSiteChangedVisible(site))
      return { label: "Виявлено зміну", cls: "sev-high" };
    if (site.acknowledged) return { label: "Переглянуто", cls: "sev-info" };
    if (site.status_code !== null)
      return { label: "Без змін", cls: "badge-ok" };
    return { label: "Очікує перевірки", cls: "badge-group" };
  }

  async function runAction(
    key: string,
    fn: () => Promise<unknown>,
    successNotice: string,
  ) {
    busyKey = key;
    try {
      await fn();
      await invalidateAll();
      notify("ok", successNotice);
    } catch (err) {
      const msg =
        err instanceof Error ? err.message : "Не вдалося виконати дію.";
      notify("error", msg);
    } finally {
      busyKey = null;
    }
  }
</script>

<svelte:head>
  <title>Огляд і моніторинг — Інтелектуальний Web-агент та ІСППР</title>
</svelte:head>

{#snippet faultItem(
  title: string,
  health: SourceHealth,
  busyId: string,
  btnText: string,
  onRecover: () => void,
)}
  <div class="fault-item">
    <div>
      <div class="fault-title">
        <span class={["badge", health.disabled ? "sev-critical" : "sev-high"]}>
          {health.disabled ? "Вимкнено" : "Пауза"}
        </span>
        <strong>{title}</strong>
        <span class="muted">({errorKindLabelUk(health.last_error_kind)})</span>
      </div>
      {#if health.last_error}
        <div class="fault-err mono">{health.last_error}</div>
      {/if}
      {#if !health.disabled && health.next_retry_at}
        <div class="muted">
          Наступна автоматична спроба: {formatDateTimeUk(health.next_retry_at)}
        </div>
      {/if}
    </div>
    <button
      type="button"
      class="btn btn-sm"
      disabled={busyKey === busyId}
      onclick={onRecover}
    >
      {btnText}
    </button>
  </div>
{/snippet}

{#snippet siteActionBtn(
  key: string,
  label: string,
  onClick: () => void,
  secondary = false,
)}
  <button
    type="button"
    class={["btn", "btn-sm", secondary && "btn-secondary"]}
    disabled={busyKey === key}
    onclick={onClick}
  >
    {label}
  </button>
{/snippet}

{#snippet articleCard(article: NewsArticle, rank: number)}
  <article class="article-card">
    <div class="article-main">
      <div class="article-meta">
        <span class="rank-pill num">#{rank}</span>
        <span class="badge badge-group">{groupLabelUk(article.group)}</span>
        <span class="feed-name">{article.feed_name}</span>
        {#if article.published}
          <span class="muted">• {article.published}</span>
        {/if}
      </div>

      <h3 class="article-heading">
        <a
          href={resolve("/article/[id]", {
            id: encodeURIComponent(article.id),
          })}
        >
          {article.title}
        </a>
      </h3>

      {#if article.summary}
        <p class="article-preview">{shortPreview(article.summary, 185)}</p>
      {/if}

      <div class="tag-row">
        {#each article.cves as cve (cve)}
          <span class="badge badge-cve">{cve}</span>
        {/each}
        {#each article.matched_keywords as kw (kw.word)}
          <span
            class="badge badge-kw"
            title={`Вага ${kw.weight}, внесок +${kw.contribution.toFixed(2)}`}
          >
            {kw.word}
            {#if kw.in_title}
              <span class="kw-loc">заг.</span>
            {/if}
          </span>
        {/each}
      </div>
    </div>

    <div class="article-score-col">
      <div class="score-title">Релевантність</div>
      <div class="score-num mono num">{article.relevance_score.toFixed(2)}</div>
      <CfMeter
        value={article.relevance_score}
        max={maxArticleScore}
        width="84px"
      />
      <a
        class="btn btn-secondary btn-sm detail-btn"
        href={resolve("/article/[id]", {
          id: encodeURIComponent(article.id),
        })}
      >
        Детальніше →
      </a>
    </div>
  </article>
{/snippet}

<!-- 1. Hero Situation Card -->
<section class={["panel", "hero-card", `tone-${modeInfo.tone}`]}>
  <div class="hero-top">
    <div class="hero-copy">
      <div class="hero-eyebrow">
        <span
          class={[
            "badge",
            `sev-${modeInfo.tone === "ok" ? "info" : modeInfo.tone}`,
          ]}
        >
          Операційний стан ІСППР
        </span>
        <span class="mono muted">{modeInfo.code}</span>
      </div>
      <h1 class="hero-title">{modeInfo.title}</h1>
      <p class="hero-summary">{modeInfo.summary}</p>
    </div>

    <div class="hero-kpis">
      <div class="kpi-tile">
        <span class="kpi-label">Зміни HTTP-сайтів</span>
        <span
          class={["kpi-val", "num", changedSites.length > 0 && "kpi-alert"]}
        >
          {changedSites.length}
          <span class="kpi-sub">/ {snapshot.sites.length}</span>
        </span>
        <span class="kpi-hint">непереглянутих оновлень</span>
      </div>

      <div class="kpi-tile">
        <span class="kpi-label">Відібрані новини</span>
        <span class="kpi-val num">{snapshot.articles.length}</span>
        <span class="kpi-hint">
          {#if totalCveArticles > 0}
            з них {totalCveArticles} із CVE
          {:else}
            за ключовими словами
          {/if}
        </span>
      </div>

      <a class="kpi-tile kpi-link" href={resolve("/dss")}>
        <span class="kpi-label">Рішення ІСППР</span>
        <span class="kpi-val num">{snapshot.inference.decisions.length}</span>
        <span class="kpi-hint">
          {snapshot.inference.fired_rule_ids.length} активних правил →
        </span>
      </a>

      <div class="kpi-tile">
        <span class="kpi-label">Цикл опитування</span>
        <span class="kpi-val num">#{snapshot.poll_count}</span>
        <span class="kpi-hint">
          {#if snapshot.last_poll_duration_ms !== null}
            {snapshot.last_poll_duration_ms} мс • {formatDateTimeUk(
              snapshot.last_poll_at,
            )}
          {:else}
            Очікування першого циклу
          {/if}
        </span>
      </div>
    </div>
  </div>

  <div class="pipeline-strip" aria-label="Схема роботи Web-агента та ІСППР">
    <div class="pipe-step">
      <span class="pipe-num">1</span>
      <div>
        <strong>Збір Web-даних</strong>
        <span>
          Агент паралельно опитує {snapshot.sites.length} HTTP-сайтів, {snapshot
            .feeds.length}
          RSS/Atom-стрічок та API НБУ.
        </span>
      </div>
    </div>
    <div class="pipe-step">
      <span class="pipe-num">2</span>
      <div>
        <strong>Ранжування та факти</strong>
        <span>
          Статті оцінюються за зваженими ключовими словами та CVE; показники
          формують
          {Object.keys(snapshot.agent_facts).length} фактів.
        </span>
      </div>
    </div>
    <div class="pipe-step">
      <span class="pipe-num">3</span>
      <div>
        <strong>Логічний вивід ІСППР</strong>
        <span>
          Продукційна машина прямого виводу формує операційний режим і
          рекомендації з поясненням.
        </span>
      </div>
    </div>
  </div>
</section>

<!-- 2. Source Fault Alerts (only if any disabled or throttled) -->
{#if hasAnyFault}
  <section class="panel fault-panel" aria-label="Попередження про стан джерел">
    <div class="panel-header">
      <div>
        <h2 class="panel-title">Увага: обмеження або збої зовнішніх джерел</h2>
        <p class="panel-desc">
          Захисний механізм (circuit breaker / експоненційний backoff)
          автоматично призупинив проблемні джерела. Вимкнено: {disabledCount}.
        </p>
      </div>
    </div>

    <div class="fault-list">
      {#each faultSites as site (site.id)}
        {@render faultItem(
          `HTTP-сайт: ${site.name}`,
          site.health,
          `en-site-${site.id}`,
          "Відновити зараз",
          () =>
            void runAction(
              `en-site-${site.id}`,
              () => enableSite(site.id),
              `Моніторинг сайту «${site.name}» відновлено.`,
            ),
        )}
      {/each}

      {#each faultFeeds as feed (feed.id)}
        {@render faultItem(
          `RSS-стрічка: ${feed.name}`,
          feed.health,
          `en-feed-${feed.id}`,
          "Відновити зараз",
          () =>
            void runAction(
              `en-feed-${feed.id}`,
              () => enableFeed(feed.id),
              `Стрічку новин «${feed.name}» знову увімкнено.`,
            ),
        )}
      {/each}

      {#if nbuHasFault}
        {@render faultItem(
          "API курсів валют НБУ",
          snapshot.nbu_health,
          "en-nbu",
          "Відновити НБУ",
          () =>
            void runAction(
              "en-nbu",
              () => enableNbu(),
              "Моніторинг курсів валют НБУ відновлено.",
            ),
        )}
      {/if}
    </div>
  </section>
{/if}

<!-- 3. Two-Column Highlight: Top DSS Decisions + NBU Currency Rates -->
<div class="split-grid">
  <section class="panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title">Пріоритетні рекомендації ІСППР</h2>
        <p class="panel-desc">
          Висновки продукційної бази знань, сформовані на основі зібраних
          фактів.
        </p>
      </div>
      <a class="btn btn-secondary btn-sm" href={resolve("/dss")}>
        Усі рішення та симулятор →
      </a>
    </div>

    {#if topDecisions.length === 0}
      <div class="empty-state">
        Наразі немає активних рекомендацій. Запустіть опитування або додайте
        факти в ІСППР.
      </div>
    {:else}
      <div class="decision-preview-list">
        {#each topDecisions as dec (dec.rule_id)}
          <div class={["decision-preview", `border-${dec.severity}`]}>
            <div class="dp-head">
              <span class={["badge", `sev-${dec.severity}`]}>
                {severityLabelUk(dec.severity)}
              </span>
              <span class="mono muted">{dec.rule_id}</span>
              <CfMeter
                value={dec.cf}
                tone={dec.severity}
                label={`CF ${dec.cf.toFixed(2)}`}
              />
            </div>
            <div class="dp-name">{dec.rule_name}</div>
            <p class="dp-rec">{dec.recommendation}</p>
          </div>
        {/each}
      </div>
      {#if snapshot.inference.decisions.length > 3}
        <div class="more-link-row">
          <a href={resolve("/dss")}>
            Показати ще {snapshot.inference.decisions.length - 3} рішень та ланцюжки
            доведення →
          </a>
        </div>
      {/if}
    {/if}
  </section>

  <section class="panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title">Офіційні курси валют НБУ</h2>
        <p class="panel-desc">
          Дані JSON API Національного банку України для фінансових правил ІСППР.
        </p>
      </div>
      {#if snapshot.currencies.length > 0}
        <span class="badge badge-ok">
          Актуально: {snapshot.currencies[0].exchangedate}
        </span>
      {/if}
    </div>

    {#if snapshot.currencies.length === 0}
      <div class="empty-state">
        Курси валют ще не завантажені або API НБУ тимчасово недоступний.
      </div>
    {:else}
      <div class="currency-grid">
        {#each snapshot.currencies as cur (cur.cc)}
          <div class="currency-card">
            <div class="cur-head">
              <span class="cur-code">{cur.cc}</span>
              {#if cur.delta_pct !== null}
                <span
                  class={[
                    "badge",
                    cur.delta_pct > 0.001
                      ? "sev-high"
                      : cur.delta_pct < -0.001
                        ? "badge-ok"
                        : "badge-group",
                  ]}
                >
                  {cur.delta_pct > 0 ? "+" : ""}{cur.delta_pct.toFixed(2)}%
                </span>
              {:else}
                <span class="badge badge-group">базове</span>
              {/if}
            </div>
            <div class="cur-rate num">
              {cur.rate.toFixed(4)} <span class="cur-unit">грн</span>
            </div>
            <div class="cur-name">{cur.txt}</div>
            {#if cur.previous_rate !== null}
              <div class="cur-prev muted num">
                Попередній: {cur.previous_rate.toFixed(4)} грн
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>

<!-- 4. HTTP/HTTPS Site Changes Monitor -->
<section class="panel">
  <div class="panel-header">
    <div>
      <h2 class="panel-title">Моніторинг змін HTTP/HTTPS-сайтів</h2>
      <p class="panel-desc">
        Агент порівнює контрольні суми (SHA-256), ETag і розмір тіла відповіді.
        Натисніть «Переглянуто», щоб підтвердити ознайомлення зі зміною.
      </p>
    </div>

    <div class="filter-pills" role="group" aria-label="Фільтр сайтів">
      <button
        type="button"
        class={["pill-btn", siteFilter === "all" && "active"]}
        onclick={() => (siteFilter = "all")}
      >
        Усі ({snapshot.sites.length})
      </button>
      <button
        type="button"
        class={["pill-btn", siteFilter === "changed" && "active"]}
        onclick={() => (siteFilter = "changed")}
      >
        Нові зміни ({changedSites.length})
      </button>
      <button
        type="button"
        class={["pill-btn", siteFilter === "ack" && "active"]}
        onclick={() => (siteFilter = "ack")}
      >
        Переглянуті ({snapshot.sites.filter((s) => s.acknowledged).length})
      </button>
    </div>
  </div>

  {#if filteredSites.length === 0}
    <div class="empty-state">За обраним фільтром HTTP/HTTPS-сайтів немає.</div>
  {:else}
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Ресурс</th>
            <th>Статус</th>
            <th>HTTP / Розмір</th>
            <th>Динаміка</th>
            <th>Останні зміни</th>
            <th>Дія</th>
          </tr>
        </thead>
        <tbody>
          {#each filteredSites as site (site.id)}
            {@const st = siteStatusBadge(site)}
            {@const delta = formatSizeDelta(site.body_size, site.previous_size)}
            <tr class={[isSiteChangedVisible(site) && "row-highlight"]}>
              <td>
                <div class="site-name">
                  <a href={site.url} target="_blank" rel="noopener">
                    {site.name} ↗
                  </a>
                </div>
                <div class="site-meta">
                  <span class="badge badge-group">
                    {groupLabelUk(site.category)}
                  </span>
                  <span class="mono muted">{site.url}</span>
                </div>
              </td>
              <td>
                <span class={["badge", st.cls]}>{st.label}</span>
                {#if site.change_count > 0}
                  <div class="muted num">
                    Змін за сесію: {site.change_count}
                  </div>
                {/if}
              </td>
              <td class="num">
                {#if site.status_code !== null}
                  <span class="mono">HTTP {site.status_code}</span>
                {:else}
                  <span class="muted">—</span>
                {/if}
                <div class="muted">{formatBytes(site.body_size)}</div>
              </td>
              <td class="num">
                <span
                  class={[
                    "delta-chip",
                    delta.kind === "up" && "delta-up",
                    delta.kind === "down" && "delta-down",
                  ]}
                >
                  {delta.text}
                </span>
              </td>
              <td class="muted">
                <div>Зміна: {formatDateTimeUk(site.last_changed)}</div>
                <div>
                  Перевірка: {formatDateTimeUk(site.health.last_checked)}
                </div>
              </td>
              <td>
                {#if site.health.disabled}
                  {@render siteActionBtn(
                    `site-en-${site.id}`,
                    "Увімкнути",
                    () =>
                      void runAction(
                        `site-en-${site.id}`,
                        () => enableSite(site.id),
                        `Моніторинг сайту «${site.name}» відновлено.`,
                      ),
                  )}
                {:else if isSiteChangedVisible(site)}
                  {@render siteActionBtn(
                    `site-ack-${site.id}`,
                    "✓ Переглянуто",
                    () =>
                      void runAction(
                        `site-ack-${site.id}`,
                        () => ackSite(site.id),
                        `Сайт «${site.name}» позначено як переглянутий.`,
                      ),
                  )}
                {:else if site.acknowledged}
                  {@render siteActionBtn(
                    `site-unack-${site.id}`,
                    "Повернути",
                    () =>
                      void runAction(
                        `site-unack-${site.id}`,
                        () => unackSite(site.id),
                        `Позначку перегляду для «${site.name}» знято.`,
                      ),
                    true,
                  )}
                {:else}
                  <span class="muted">—</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<!-- 5. Ranked News Explorer -->
<section class="panel">
  <div class="panel-header">
    <div>
      <h2 class="panel-title">Ранжовані новини з RSS/Atom-стрічок</h2>
      <p class="panel-desc">
        Автоматично відібрані повідомлення за зваженими критеріями пошуку.
        Натисніть на заголовок, щоб переглянути повний розбір оцінки
        релевантності та текст статті.
      </p>
    </div>
    <span class="muted num">
      Показано {shownArticles.length} із {filteredArticles.length} (усього {snapshot
        .articles.length})
    </span>
  </div>

  <div class="news-controls">
    <input
      type="search"
      class="search-input"
      placeholder="Пошук за заголовком, ключовим словом, CVE або джерелом…"
      bind:value={searchQuery}
      aria-label="Пошук серед відібраних новин"
    />

    <label class="toggle-check">
      <input type="checkbox" bind:checked={onlyCve} />
      <span>Лише з CVE ({totalCveArticles})</span>
    </label>

    <label class="sort-label">
      <span class="muted">Сортування:</span>
      <select class="select-input" bind:value={sortMode}>
        <option value="score">За релевантністю ↓</option>
        <option value="date">Спочатку новіші</option>
      </select>
    </label>
  </div>

  <div
    class="filter-pills group-pills"
    role="group"
    aria-label="Фільтр за темою новин"
  >
    <button
      type="button"
      class={["pill-btn", selectedGroup === "all" && "active"]}
      onclick={() => (selectedGroup = "all")}
    >
      Усі теми ({snapshot.articles.length})
    </button>
    {#each articleGroups as [grp, count] (grp)}
      <button
        type="button"
        class={["pill-btn", selectedGroup === grp && "active"]}
        onclick={() => (selectedGroup = grp)}
      >
        {groupLabelUk(grp)} ({count})
      </button>
    {/each}
  </div>

  {#if filteredArticles.length === 0}
    <div class="empty-state">
      За вашим запитом релевантних новин не знайдено. Спробуйте очистити
      фільтри.
    </div>
  {:else}
    <div class="article-list">
      {#each shownArticles as article, idx (article.id)}
        {@render articleCard(article, idx + 1)}
      {/each}
    </div>

    {#if filteredArticles.length > visibleArticlesLimit}
      <div class="load-more-row">
        <button
          type="button"
          class="btn btn-secondary"
          onclick={() => (visibleArticlesLimit += 20)}
        >
          Показати ще {Math.min(
            20,
            filteredArticles.length - visibleArticlesLimit,
          )} новин
        </button>
        <button
          type="button"
          class="btn btn-secondary"
          onclick={() => (visibleArticlesLimit = filteredArticles.length)}
        >
          Показати всі ({filteredArticles.length})
        </button>
      </div>
    {/if}
  {/if}
</section>

<!-- 6. Feed Health & Recent Agent Activity Log -->
<div class="split-grid">
  <section class="panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title">Стан RSS/Atom-стрічок</h2>
        <p class="panel-desc">
          Кількість відібраних матеріалів та технічний стан кожного джерела.
        </p>
      </div>
    </div>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Стрічка</th>
            <th>Статей</th>
            <th>Стан</th>
            <th>Перевірено</th>
          </tr>
        </thead>
        <tbody>
          {#each snapshot.feeds as feed (feed.id)}
            <tr>
              <td>
                <div class="feed-title">{feed.name}</div>
                <span class="badge badge-group">
                  {groupLabelUk(feed.group)}
                </span>
              </td>
              <td class="mono num">{feed.matched_count}</td>
              <td>
                {#if feed.health.disabled}
                  <span class="badge sev-critical">Вимкнено</span>
                {:else if isHealthThrottled(feed.health)}
                  <span class="badge sev-high">Пауза</span>
                {:else if feed.health.last_checked}
                  <span class="badge badge-ok">Активна</span>
                {:else}
                  <span class="badge badge-group">Очікує</span>
                {/if}
              </td>
              <td class="muted">
                {formatDateTimeUk(feed.health.last_checked)}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <section class="panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title">Журнал останніх подій агента</h2>
        <p class="panel-desc">
          Хронологія опитувань, виявлених змін і кроків логічного виводу
          (зберігається також у файлі журналу).
        </p>
      </div>
    </div>

    {#if snapshot.recent_logs.length === 0}
      <div class="empty-state">Записів журналу ще немає.</div>
    {:else}
      <ol class="log-list">
        {#each snapshot.recent_logs.slice(0, 12) as entry, i (`${entry.timestamp}-${i}`)}
          <li class="log-item">
            <span class="log-time mono">
              {formatDateTimeUk(entry.timestamp)}
            </span>
            <span class="log-msg">{entry.message}</span>
          </li>
        {/each}
      </ol>
    {/if}
  </section>
</div>

<style>
  .hero-card {
    border-left: 5px solid var(--accent);
  }

  .hero-card.tone-critical {
    border-left-color: var(--critical-solid);
    background: linear-gradient(180deg, #fffafa 0%, #ffffff 65%);
  }

  .hero-card.tone-high {
    border-left-color: var(--high-solid);
    background: linear-gradient(180deg, #fffbf5 0%, #ffffff 65%);
  }

  .hero-card.tone-medium {
    border-left-color: var(--medium-solid);
    background: linear-gradient(180deg, #fefdf2 0%, #ffffff 65%);
  }

  .hero-card.tone-ok {
    border-left-color: var(--ok-solid);
    background: linear-gradient(180deg, #f3fcf8 0%, #ffffff 65%);
  }

  .hero-top {
    display: grid;
    grid-template-columns: minmax(0, 1.15fr) minmax(0, 1.35fr);
    gap: 1.25rem;
    align-items: start;
  }

  .hero-eyebrow {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    margin-bottom: 0.45rem;
  }

  .hero-title {
    margin: 0;
    font-size: 1.5rem;
    line-height: 1.25;
    letter-spacing: -0.015em;
  }

  .hero-summary {
    margin: 0.5rem 0 0;
    color: var(--text-secondary);
    font-size: 0.94rem;
    max-width: 60ch;
  }

  .hero-kpis {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.7rem;
  }

  .kpi-tile {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.75rem 0.95rem;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    text-decoration: none;
    color: inherit;
  }

  .kpi-link:hover {
    border-color: var(--accent-border);
    background: var(--accent-soft);
    color: inherit;
  }

  .kpi-label {
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--muted);
  }

  .kpi-val {
    font-size: 1.4rem;
    font-weight: 800;
    line-height: 1.2;
    color: var(--text);
  }

  .kpi-val.kpi-alert {
    color: var(--high-fg);
  }

  .kpi-sub {
    font-size: 0.92rem;
    font-weight: 600;
    color: var(--muted);
  }

  .kpi-hint {
    font-size: 0.78rem;
    color: var(--muted);
  }

  .pipeline-strip {
    margin-top: 1.15rem;
    padding-top: 1rem;
    border-top: 1px solid var(--border);
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.85rem;
  }

  .pipe-step {
    display: flex;
    gap: 0.65rem;
    align-items: flex-start;
    font-size: 0.84rem;
    color: var(--text-secondary);
  }

  .pipe-step strong {
    display: block;
    color: var(--text);
    margin-bottom: 0.12rem;
  }

  .pipe-num {
    width: 1.55rem;
    height: 1.55rem;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
    border: 1px solid var(--accent-border);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 0.78rem;
    font-weight: 800;
    flex-shrink: 0;
    margin-top: 0.1rem;
  }

  .fault-panel {
    border-color: var(--high-border);
    background: #fffaf5;
  }

  .fault-list {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }

  .fault-item {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 0.95rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .fault-title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem;
  }

  .fault-err {
    margin-top: 0.25rem;
    color: var(--critical-fg);
    word-break: break-word;
  }

  .split-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1.25rem;
    align-items: start;
  }

  .decision-preview-list {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }

  .decision-preview {
    padding: 0.85rem 1rem;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-left: 4px solid var(--info-solid);
    border-radius: var(--radius-md);
  }

  .decision-preview.border-critical {
    border-left-color: var(--critical-solid);
  }

  .decision-preview.border-high {
    border-left-color: var(--high-solid);
  }

  .decision-preview.border-medium {
    border-left-color: var(--medium-solid);
  }

  .dp-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.3rem;
  }

  .dp-name {
    font-weight: 700;
    font-size: 0.94rem;
    margin-bottom: 0.2rem;
  }

  .dp-rec {
    margin: 0;
    font-size: 0.86rem;
    color: var(--text-secondary);
  }

  .more-link-row {
    margin-top: 0.8rem;
    font-size: 0.86rem;
    font-weight: 600;
  }

  .currency-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.75rem;
  }

  .currency-card {
    padding: 0.85rem 1rem;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .cur-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.4rem;
  }

  .cur-code {
    font-size: 0.92rem;
    font-weight: 800;
    letter-spacing: 0.03em;
  }

  .cur-rate {
    margin-top: 0.35rem;
    font-size: 1.35rem;
    font-weight: 800;
    color: var(--text);
  }

  .cur-unit {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--muted);
  }

  .cur-name {
    margin-top: 0.15rem;
    font-size: 0.82rem;
    color: var(--text-secondary);
  }

  .cur-prev {
    margin-top: 0.2rem;
    font-size: 0.76rem;
  }

  .filter-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .group-pills {
    margin-bottom: 1rem;
  }

  .pill-btn {
    background: var(--surface-subtle);
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    padding: 0.32rem 0.75rem;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
    transition:
      background 0.15s,
      border-color 0.15s,
      color 0.15s;
  }

  .pill-btn:hover {
    background: #e2e8f0;
  }

  .pill-btn.active {
    background: var(--accent);
    border-color: var(--accent);
    color: #ffffff;
  }

  .row-highlight td {
    background: #fffbf2;
  }

  .site-name {
    font-weight: 600;
    margin-bottom: 0.2rem;
  }

  .site-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
    word-break: break-all;
  }

  .delta-chip {
    display: inline-block;
    padding: 0.15rem 0.48rem;
    border-radius: var(--radius-sm);
    font-size: 0.8rem;
    font-weight: 600;
    background: var(--surface-subtle);
    color: var(--text-secondary);
    border: 1px solid var(--border);
  }

  .delta-up {
    background: var(--ok-bg);
    color: var(--ok-fg);
    border-color: var(--ok-border);
  }

  .delta-down {
    background: var(--high-bg);
    color: var(--high-fg);
    border-color: var(--high-border);
  }

  .news-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }

  .search-input {
    flex: 1 1 280px;
    padding: 0.48rem 0.8rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    font-size: 0.88rem;
    font-family: inherit;
    background: var(--surface);
    color: var(--text);
  }

  .toggle-check {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .sort-label {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }

  .select-input {
    padding: 0.42rem 0.65rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    font-size: 0.84rem;
    font-family: inherit;
  }

  .article-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .article-card {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 150px;
    gap: 1rem;
    padding: 1rem 1.1rem;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    transition: border-color 0.15s;
  }

  .article-card:hover {
    border-color: var(--accent-border);
  }

  .article-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem;
    font-size: 0.82rem;
    margin-bottom: 0.3rem;
  }

  .rank-pill {
    font-weight: 800;
    color: var(--accent);
    font-size: 0.82rem;
  }

  .feed-name {
    font-weight: 600;
    color: var(--text-secondary);
  }

  .article-heading {
    margin: 0;
    font-size: 1.02rem;
    line-height: 1.35;
  }

  .article-heading a {
    color: var(--text);
    text-decoration: none;
  }

  .article-heading a:hover {
    color: var(--accent);
    text-decoration: underline;
  }

  .article-preview {
    margin: 0.35rem 0 0.55rem;
    font-size: 0.87rem;
    color: var(--text-secondary);
  }

  .tag-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .kw-loc {
    font-size: 0.68rem;
    opacity: 0.75;
    margin-left: 0.15rem;
  }

  .article-score-col {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    justify-content: space-between;
    gap: 0.35rem;
    padding-left: 0.85rem;
    border-left: 1px solid var(--border);
  }

  .score-title {
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--muted);
  }

  .score-num {
    font-size: 1.3rem;
    font-weight: 800;
    color: var(--text);
  }

  .detail-btn {
    margin-top: 0.35rem;
  }

  .load-more-row {
    margin-top: 1rem;
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.6rem;
  }

  .feed-title {
    font-weight: 600;
    margin-bottom: 0.2rem;
  }

  .log-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  .log-item {
    display: flex;
    flex-direction: column;
    gap: 0.12rem;
    padding: 0.52rem 0.75rem;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-size: 0.84rem;
  }

  .log-time {
    font-size: 0.74rem;
    color: var(--muted);
  }

  .log-msg {
    color: var(--text-secondary);
    word-break: break-word;
  }

  @media (max-width: 960px) {
    .hero-top,
    .pipeline-strip,
    .split-grid {
      grid-template-columns: 1fr;
    }

    .article-card {
      grid-template-columns: 1fr;
    }

    .article-score-col {
      align-items: flex-start;
      border-left: none;
      border-top: 1px solid var(--border);
      padding-left: 0;
      padding-top: 0.7rem;
    }
  }
</style>
