<script lang="ts">
	import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
	import type { Component } from 'svelte';

	/** Test-only: renders `component` with `props` under a fresh QueryClient (no retries). */
	// eslint-disable-next-line @typescript-eslint/no-explicit-any -- any component; its props are the test's business
	let { component: C, props = {} }: { component: Component<any>; props?: Record<string, unknown> } =
		$props();
	const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
</script>

<QueryClientProvider {client}><C {...props} /></QueryClientProvider>
