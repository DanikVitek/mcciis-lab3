<script lang="ts">
  import { invalidateAll } from '$app/navigation';
  import { resolve } from '$app/paths';
  import CfMeter from '#lib/components/CfMeter.svelte';
  import ErrorBanner from '#lib/components/ErrorBanner.svelte';
  import LoadingSkeleton from '#lib/components/LoadingSkeleton.svelte';
  import Tooltip from '#lib/components/Tooltip.svelte';
  import { groupLabelUk, parseSummaryBlocks } from '#lib/format.ts';
  import type { NewsArticle } from '#lib/schemas.ts';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();

  function computeBreakdown(article: NewsArticle) {
    const kwSubtotal = article.matched_keywords.reduce((sum, kw) => sum + kw.contribution, 0);
    const diversityBonus = Math.max(0, article.matched_keywords.length - 1) * 0.75;
    const cveBonus = article.cves.length > 0 ? 2.0 : 0.0;
    return {
      kwSubtotal,
      diversityBonus,
      cveBonus,
      total: article.relevance_score,
    };
  }
</script>

<svelte:head>
  {#await data.articlePromise}
    <title>Завантаження новини — Web-агент та ІСППР</title>
  {:then article}
    <title>{article ? article.title : 'Новину не знайдено'} — Web-агент та ІСППР</title>
  {:catch}
    <title>Помилка завантаження — Web-агент та ІСППР</title>
  {/await}
</svelte:head>

<div class="back-row">
  <a class="btn btn-secondary btn-sm" href={resolve('/')}>
    ← Повернутися до списку новин
  </a>
</div>

{#await data.articlePromise}
  <LoadingSkeleton variant="article" message="Завантаження деталей статті та розбору рейтингу…" />
{:then article}
  {#if !article}
    <section class="panel not-found-panel">
      <span class="badge sev-medium">HTTP 404</span>
      <h2 class="nf-title">Новину з таким ідентифікатором не знайдено</h2>
      <p class="muted">
        Стаття могла бути витіснена під час наступного циклу опитування стрічки новин або
        посилання застаріло.
      </p>
      <a class="btn btn-sm" href={resolve('/')}>Перейти до актуального списку новин</a>
    </section>
  {:else}
    {@const parsed = parseSummaryBlocks(article.summary)}
    {@const breakdown = computeBreakdown(article)}

    <article class="panel">
      <div class="article-header">
        <div>
          <div class="meta-pills">
            <span class="badge badge-group">{groupLabelUk(article.group)}</span>
            <span class="muted">Джерело: <strong>{article.feed_name}</strong></span>
            {#if article.published}
              <span class="muted">• Опубліковано: {article.published}</span>
            {/if}
          </div>
          <h2 class="article-title">{article.title}</h2>
        </div>

        <div class="score-badge-box">
          <div class="score-label-row">
            <span class="score-label">Оцінка релевантності</span>
            <Tooltip
              title="Рейтинг релевантності"
              text="Зважена сума знайдених ключових слів (заголовок ×2, текст ×1) плюс бонус за різноманіття тем і виявлені ідентифікатори CVE."
              align="end"
            />
          </div>
          <div class="score-big mono num">{article.relevance_score.toFixed(2)}</div>
        </div>
      </div>

      <div class="cve-row">
        <span class="cve-label">Виявлені ідентифікатори CVE:</span>
        <Tooltip
          title="Автоматичний сканер CVE"
          text="Агент автоматично виявляє унікальні ідентифікатори формату CVE-YYYY-NNNN у заголовку й тексті статті, додає +2.0 до рейтингу та формує факт cve_mentions_count для ІСППР."
          align="start"
        />
        {#if article.cves.length === 0}
          <span class="muted">Не виявлено</span>
        {:else}
          {#each article.cves as cve (cve)}
            <a
              class="badge badge-cve cve-link"
              href={`https://nvd.nist.gov/vuln/detail/${encodeURIComponent(cve)}`}
              target="_blank"
              rel="noopener"
              title="Відкрити запис у базі NVD"
            >
              {cve} ↗
            </a>
          {/each}
        {/if}
      </div>

      <div class="article-body">
        {#if parsed.sections.length === 0}
          <p class="muted">Повний текст опису відсутній у стрічці новин.</p>
        {:else}
          {#each parsed.sections as sec, i (`sec-${i}`)}
            {#if sec.heading}
              <h3>{sec.heading}</h3>
            {/if}
            {#each sec.paragraphs as para, j (`p-${i}-${j}`)}
              <p>{para}</p>
            {/each}
          {/each}

          {#if parsed.references.length > 0}
            <div class="refs">
              <strong>Посилання з бюлетеня:</strong>
              <ul>
                {#each parsed.references as ref (ref.index)}
                  <li>
                    <span class="mono">[{ref.index}]</span>
                    <a href={ref.url} target="_blank" rel="noopener">{ref.url}</a>
                  </li>
                {/each}
              </ul>
            </div>
          {/if}
        {/if}
      </div>

      {#if article.link}
        <div class="orig-row">
          <a class="btn" href={article.link} target="_blank" rel="noopener">
            Перейти до оригіналу статті ↗
          </a>
        </div>
      {/if}
    </article>

    <section class="panel">
      <div class="panel-header">
        <div>
          <h3 class="panel-title title-with-tip">
            Як сформовано оцінку релевантності
            <Tooltip
              title="Повна прозорість ранжування"
              text="Кожен доданок рейтингу пояснюється окремо: базові ваги ключових слів, коефіцієнт розташування, бонус різноманіття та бонус CVE."
              align="start"
            />
          </h3>
          <p class="panel-desc">
            Прозора формула ранжування: ключові слова (у заголовку ×2, у тексті ×1) + бонус за
            різноманіття ключових слів + бонус за згадку CVE.
          </p>
        </div>
      </div>

      <div class="formula-grid">
        <div class="formula-box">
          <div class="k">Сума ключових слів ({article.matched_keywords.length})</div>
          <div class="v mono num">+{breakdown.kwSubtotal.toFixed(2)}</div>
        </div>
        <div class="formula-box">
          <div class="k">Бонус за різноманіття (0.75 × (|M| − 1))</div>
          <div class="v mono num">+{breakdown.diversityBonus.toFixed(2)}</div>
        </div>
        <div class="formula-box">
          <div class="k">Бонус за виявлені CVE</div>
          <div class="v mono num">+{breakdown.cveBonus.toFixed(2)}</div>
        </div>
        <div class="formula-box total">
          <div class="k">Підсумковий рейтинг</div>
          <div class="v mono num">{breakdown.total.toFixed(2)}</div>
        </div>
      </div>

      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Ключове слово</th>
              <th>Вага</th>
              <th>У заголовку (×2)</th>
              <th>У тексті (×1)</th>
              <th>Внесок</th>
            </tr>
          </thead>
          <tbody>
            {#each article.matched_keywords as kw (kw.word)}
              <tr>
                <td><span class="badge badge-kw">{kw.word}</span></td>
                <td class="mono num">{kw.weight.toFixed(2)}</td>
                <td>
                  {#if kw.in_title}
                    <span class="badge badge-ok">Так (×2)</span>
                  {:else}
                    <span class="muted">Ні</span>
                  {/if}
                </td>
                <td>
                  {#if kw.in_body}
                    <span class="badge badge-ok">Так (×1)</span>
                  {:else}
                    <span class="muted">Ні</span>
                  {/if}
                </td>
                <td>
                  <CfMeter
                    value={kw.contribution}
                    max={Math.max(breakdown.kwSubtotal, 1)}
                    tone="accent"
                    label={`+${kw.contribution.toFixed(2)}`}
                  />
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/if}
{:catch err}
  <ErrorBanner
    error={err}
    title="Не вдалося завантажити статтю"
    onRetry={() => void invalidateAll()}
  />
{/await}

<style>
  .title-with-tip {
    display: inline-flex;
    align-items: center;
    gap: 0.42rem;
  }

  .back-row {
    margin-bottom: 1rem;
  }

  .not-found-panel {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.65rem;
  }

  .nf-title {
    margin: 0;
    font-size: 1.2rem;
  }

  .article-header {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    margin-bottom: 1rem;
  }

  .meta-pills {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem;
    margin-bottom: 0.45rem;
  }

  .article-title {
    margin: 0;
    font-size: 1.42rem;
    line-height: 1.3;
    max-width: 56ch;
  }

  .score-badge-box {
    background: var(--accent-soft);
    border: 1px solid var(--accent-border);
    border-radius: var(--radius-md);
    padding: 0.65rem 1rem;
    text-align: right;
    flex-shrink: 0;
  }

  .score-label-row {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }

  .score-label {
    font-size: 0.74rem;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--info-fg);
  }

  .score-big {
    font-size: 1.55rem;
    font-weight: 800;
    color: var(--text);
  }

  .cve-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem;
    padding: 0.65rem 0.9rem;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    margin-bottom: 1rem;
  }

  .cve-label {
    font-size: 0.85rem;
    font-weight: 600;
  }

  .cve-link {
    text-decoration: none;
  }

  .cve-link:hover {
    text-decoration: underline;
  }

  .article-body {
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 1.25rem 1.45rem;
    line-height: 1.68;
    color: var(--text);
  }

  .article-body h3 {
    margin: 1.1rem 0 0.35rem;
    font-size: 1rem;
    color: var(--text);
  }

  .article-body h3:first-child {
    margin-top: 0;
  }

  .article-body p {
    margin: 0.45rem 0;
  }

  .refs {
    margin-top: 1.1rem;
    padding-top: 0.85rem;
    border-top: 1px dashed var(--border-strong);
    font-size: 0.84rem;
  }

  .refs ul {
    margin: 0.4rem 0 0;
    padding-left: 1.2rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .refs a {
    word-break: break-all;
  }

  .orig-row {
    margin-top: 1rem;
  }

  .formula-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.75rem;
    margin-bottom: 1rem;
  }

  .formula-box {
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 0.8rem 0.95rem;
  }

  .formula-box.total {
    background: var(--accent-soft);
    border-color: var(--accent-border);
  }

  .formula-box .k {
    font-size: 0.76rem;
    color: var(--muted);
    font-weight: 600;
  }

  .formula-box .v {
    margin-top: 0.25rem;
    font-size: 1.22rem;
    font-weight: 800;
  }

  @media (max-width: 900px) {
    .formula-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
