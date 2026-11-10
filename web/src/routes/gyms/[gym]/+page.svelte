<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import PageHeader from '#lib/components/page-header.svelte';
	import ReadOnlyNote from '#lib/components/read-only-note.svelte';
	import GymEquipment from '#lib/components/gym-equipment.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import type { Gym } from '#lib/api/results.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import { splitList } from '#lib/utils.js';

	const gym = toolQuery<Gym, 'gym_get'>('gym_get', () => ({ gym: page.params.gym ?? '' }));
	const update = toolMutation<Gym, 'gym_update'>('gym_update', {
		invalidate: ['gym_get', 'gym_list'],
		success: 'Gym saved'
	});

	let open = $state(false);
	let name = $state('');
	let aliases = $state('');
	let notes = $state('');

	function edit(g: Gym) {
		name = g.name;
		aliases = g.aliases.join(', ');
		notes = g.notes ?? '';
		open = true;
	}

	function submit(ev: SubmitEvent) {
		ev.preventDefault();
		update.mutate(
			{
				gym: page.params.gym ?? '',
				name: name.trim(),
				aliases: splitList(aliases),
				notes: notes.trim()
			},
			{ onSuccess: () => (open = false) }
		);
	}
</script>

<a href={resolve('/gyms')} class="mb-2 inline-block text-sm text-muted-foreground hover:underline"
	>← All gyms</a
>
{#if gym.isPending}
	<Skeleton class="mb-4 h-8 w-64" />
	<Skeleton class="h-32 w-full" />
{:else if gym.isError}
	<p class="text-sm text-destructive">Couldn’t load this gym: {gym.error.message}</p>
{:else}
	{@const g = gym.data}
	<PageHeader
		title={g.name}
		description={g.aliases.length ? `Also called ${g.aliases.join(', ')}` : undefined}
	>
		{#snippet actions()}
			{#if logbook.canWrite('gym')}
				<Button variant="outline" onclick={() => edit(g)}>Edit gym</Button>
			{/if}
		{/snippet}
	</PageHeader>
	<ReadOnlyNote domain="gym" />
	<div class="grid max-w-3xl gap-8">
		<GymEquipment gym={g} editable={logbook.canWrite('gym')} />
		<section class="grid gap-3">
			<h2 class="text-lg font-semibold">Notes</h2>
			{#if g.notes}
				<p class="text-sm whitespace-pre-line">{g.notes}</p>
			{:else}
				<p class="text-sm text-muted-foreground">
					No notes. Opening hours or when it's busy go here.
				</p>
			{/if}
		</section>
	</div>
{/if}

<Dialog.Root bind:open>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>Edit gym</Dialog.Title>
		</Dialog.Header>
		<form class="grid gap-4" onsubmit={submit}>
			<div class="grid gap-2">
				<Label for="gym-name">Name</Label>
				<Input id="gym-name" required bind:value={name} />
			</div>
			<div class="grid gap-2">
				<Label for="gym-aliases">Other names</Label>
				<Input id="gym-aliases" bind:value={aliases} />
				<p class="text-xs text-muted-foreground">Separate with commas.</p>
			</div>
			<div class="grid gap-2">
				<Label for="gym-notes">Notes</Label>
				<Textarea id="gym-notes" bind:value={notes} />
			</div>
			<Dialog.Footer>
				<Button type="submit" disabled={update.isPending}>Save gym</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
