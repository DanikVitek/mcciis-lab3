import { fetchUiState } from '#lib/api.ts';
import type { LayoutLoad } from './$types';

export const ssr = false;

export const load: LayoutLoad = async ({ fetch }) => {
  const uiState = await fetchUiState(fetch);
  return { uiState };
};
