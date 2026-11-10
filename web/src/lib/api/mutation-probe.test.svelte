<script lang="ts">
	import { setQueryClientContext, type QueryClient } from '@tanstack/svelte-query';
	import { untrack } from 'svelte';
	import { toolMutation } from './query.svelte.js';

	/** Test-only: runs toolMutation in a component and hands it to the test. */
	let {
		client,
		ready
	}: {
		client: QueryClient;
		ready: (m: ReturnType<typeof toolMutation<unknown, 'gym_create'>>) => void;
	} = $props();
	untrack(() => {
		setQueryClientContext(client);
		ready(toolMutation('gym_create', { invalidate: ['gym_list'] }));
	});
</script>
