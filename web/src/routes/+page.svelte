<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import * as Card from '#lib/components/ui/card/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { tool } from '#lib/api/client.js';
	import { logbook } from '#lib/logbook.svelte.js';

	// The first real data on the home page, through the same tools Claude uses.
	const gyms = createQuery(() => ({
		queryKey: [logbook.owner, 'gym_list', {}],
		queryFn: () => tool('gym_list', {}, { owner: logbook.owner }) as Promise<unknown[]>,
		enabled: logbook.canRead('gym')
	}));
</script>

<div class="grid max-w-xl gap-4">
	<h1 class="text-2xl font-semibold tracking-tight">
		{logbook.owner ? `Logbook of ${logbook.owner}` : 'Your logbook'}
	</h1>
	<Card.Root>
		<Card.Header>
			<Card.Title>Gyms</Card.Title>
			<Card.Description>Where workouts are logged.</Card.Description>
		</Card.Header>
		<Card.Content>
			{#if !logbook.canRead('gym')}
				<p class="text-muted-foreground">This logbook's gym data isn't shared with you.</p>
			{:else if gyms.isPending}
				<Skeleton class="h-8 w-24" />
			{:else if gyms.isError}
				<p class="text-destructive">{gyms.error.message}</p>
			{:else}
				<p class="font-numerals text-3xl tabular-nums">{gyms.data.length}</p>
			{/if}
		</Card.Content>
	</Card.Root>
</div>
