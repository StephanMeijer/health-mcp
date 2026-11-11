import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/svelte';
import { afterEach } from 'vitest';

// Without vitest globals, testing-library can't register its own cleanup: unmount between tests here.
afterEach(cleanup);

// jsdom has no ResizeObserver; `bind:clientWidth` and the charts need one to mount.
globalThis.ResizeObserver ??= class {
	observe() {}
	unobserve() {}
	disconnect() {}
};
