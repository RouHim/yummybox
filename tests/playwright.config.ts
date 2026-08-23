import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
	testDir: './e2e',
	// Container-gate mode (YUMMYBOX_NO_WEBSERVER=1) runs the suite against the
	// Dockerized app; the container cannot reach the host-loopback mock LLM on
	// 127.0.0.1:18999, so skip the mock-dependent generate-meal spec there.
	// Full E2E coverage of the generate feature runs in the host e2e job.
	testIgnore: process.env.YUMMYBOX_NO_WEBSERVER === '1' ? '**/generate-meal.spec.ts' : undefined,
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 2 : 0,
	workers: 1,
	reporter: process.env.CI
		? [['github'], ['junit', { outputFile: 'results.xml' }], ['html', { open: 'never' }]]
		: [['list'], ['html', { open: 'on-failure' }]],
	timeout: 30_000,
	expect: { timeout: 5_000 },
	use: {
		baseURL: 'http://127.0.0.1:11342',
		actionTimeout: 10_000,
		navigationTimeout: 15_000,
		trace: 'on-first-retry',
		screenshot: 'only-on-failure',
		video: 'retain-on-failure',
	},
	projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
	webServer: process.env.YUMMYBOX_NO_WEBSERVER === '1' ? undefined : [
		{
			command: "bash -c 'mkdir -p .e2e-db && if [ -x target/x86_64-unknown-linux-gnu/release/yummybox ]; then exec target/x86_64-unknown-linux-gnu/release/yummybox; elif [ -x target/release/yummybox ]; then exec target/release/yummybox; else exec cargo run --quiet; fi'",
			cwd: '..',
			url: 'http://127.0.0.1:11342/api/meals',
			reuseExistingServer: !process.env.CI,
			timeout: 60_000,
			stdout: 'pipe',
			stderr: 'pipe',
			env: {
				YUMMYBOX_PORT: '11342',
				YUMMYBOX_DATA_DIR: './.e2e-db',
			},
		},
		{
			command: 'node tests/e2e/mock-llm.mjs',
			cwd: '..',
			url: 'http://127.0.0.1:18999/v1/models',
			reuseExistingServer: !process.env.CI,
			timeout: 30_000,
		},
	],
});
