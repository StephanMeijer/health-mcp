import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/svelte';
import { afterEach } from 'vitest';

// Without vitest globals, testing-library can't register its own cleanup: unmount between tests here.
afterEach(cleanup);
