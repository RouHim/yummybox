import { defineConfig } from '@playwright/test';

export default defineConfig({
	testDir: './e2e',
	timeout: 15000,
	retries: 0,
	use: {
		baseURL: 'http://127.0.0.1:11341',
		headless: true,
	},
	webServer: {
		command: 'test -f ./target/x86_64-unknown-linux-gnu/release/yummybox && ./target/x86_64-unknown-linux-gnu/release/yummybox || ./target/release/yummybox',
		cwd: '..',
		timeout: 30000,
		reuseExistingServer: !process.env.CI,
	},
});
