import { fetchArticleById } from '#lib/api.ts';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ fetch, params }) => {
  return {
    articlePromise: fetchArticleById(params.id, fetch),
  };
};
