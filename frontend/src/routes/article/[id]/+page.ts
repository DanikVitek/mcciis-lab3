import { error } from '@sveltejs/kit';
import { fetchArticleById } from '#lib/api.ts';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
  const article = await fetchArticleById(fetch, params.id);
  if (!article) {
    error(404, 'Новину з таким ідентифікатором не знайдено.');
  }
  return { article };
};
