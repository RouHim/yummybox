import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	// Mirrors the `$lib` alias declared in vite.config.ts; Kit 3 no longer
	// injects it implicitly and this config builds its own plugin instance.
	plugins: [sveltekit({ alias: { $lib: 'src/lib' } })],
	test: {
		include: ['src/**/*.test.ts']
	}
});
