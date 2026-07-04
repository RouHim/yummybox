import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
	testDir: './e2e',
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 2 : 0,
	workers: process.env.CI ? 1 : undefined,
	reporter: process.env.CI
		? [['github'], ['junit', { outputFile: 'results.xml' }], ['html', { open: 'never' }]]
		: [['list'], ['html', { open: 'on-failure' }]],
	timeout: 30_000,
	expect: { timeout: 5_000 },
	use: {
		baseURL: 'http://localhost:11342',
		actionTimeout: 10_000,
		navigationTimeout: 15_000,
		trace: 'on-first-retry',
		screenshot: 'only-on-failure',
		video: 'retain-on-failure',
	},
	projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
	webServer: {
		command: "bash -c 'mkdir -p .e2e-db && if [ -x target/release/yummybox ]; then exec target/release/yummybox; else exec cargo run --quiet; fi'",
		cwd: '..',
		url: 'http://localhost:11342/api/meals',
		reuseExistingServer: !process.env.CI,
		timeout: 60_000,
		stdout: 'pipe',
		stderr: 'pipe',
		env: {
			YUMMYBOX_PORT: '11342',
			YUMMYBOX_DATA_DIR: './.e2e-db',
		},
	},
});
