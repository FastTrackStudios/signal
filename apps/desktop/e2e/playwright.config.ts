import { defineConfig } from '@playwright/test';

// baseURL is set by global-setup.ts (it picks a free scratch port and spawns
// `target/release/fasttrackstudio --engine` on it). Workers re-evaluate this
// file after globalSetup has exported FTS_E2E_BASEURL into the environment.
export default defineConfig({
  testDir: './tests',
  globalSetup: './global-setup.ts',
  globalTeardown: './global-teardown.ts',
  workers: 1,
  fullyParallel: false,
  retries: 0,
  // Pack streaming from the real library can take a while on first boot.
  timeout: 240_000,
  reporter: [['list']],
  outputDir: 'test-results',
  projects: [
    {
      name: 'chromium',
      use: {
        browserName: 'chromium',
        baseURL: process.env.FTS_E2E_BASEURL,
        launchOptions: {
          // THE point of this suite: the page-side AudioContext must run
          // without a user gesture so masterPeak() can prove audio out.
          args: ['--autoplay-policy=no-user-gesture-required'],
        },
      },
    },
  ],
});
