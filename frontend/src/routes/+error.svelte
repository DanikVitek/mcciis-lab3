<script lang="ts">
    import { resolve } from '$app/paths';
  import { page } from '$app/state';
</script>

<svelte:head>
  <title>Помилка {page.status} — Web-агент та ІСППР</title>
</svelte:head>

<section class="panel error-panel">
  <span class="badge sev-critical mono">HTTP {page.status}</span>
  <h1>
    {#if page.status === 404}
      Сторінку або запис не знайдено
    {:else}
      Виникла помилка під час завантаження даних
    {/if}
  </h1>
  <p class="muted">
    {page.error?.message || 'Не вдалося обробити запит. Спробуйте оновити сторінку або повернутися на головну.'}
  </p>
  <div class="actions">
    <a class="btn" href={resolve("/")}>← На головну панель</a>
    <a class="btn btn-secondary" href={resolve("/dss")}>Відкрити ІСППР</a>
  </div>
</section>

<style>
  .error-panel {
    max-width: 680px;
    margin: 2rem auto;
  }

  h1 {
    margin: 0.7rem 0 0.45rem;
    font-size: 1.4rem;
  }

  .actions {
    margin-top: 1.15rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }
</style>
