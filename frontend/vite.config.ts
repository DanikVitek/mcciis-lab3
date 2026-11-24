import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { defineConfig, loadEnv } from 'vite';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '.', '');
  const apiTarget = env.API_URL || 'http://127.0.0.1:8080';
  const host = env.FRONTEND_HOST || '127.0.0.1';
  const port = Number(env.FRONTEND_PORT || 5173);

  return {
    plugins: [
      sveltekit({
        adapter: adapter({
          pages: 'build',
          assets: 'build',
          fallback: 'index.html',
          precompress: false,
          strict: true,
        }),
        preprocess: vitePreprocess(),
        compilerOptions: {
          experimental: {
            async: true,
          },
        },
      }),
    ],
    server: {
      host,
      port,
      strictPort: true,
      proxy: {
        '/api': {
          target: apiTarget,
          changeOrigin: true,
        },
      },
    },
  };
});
