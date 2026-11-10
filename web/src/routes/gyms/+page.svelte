<script lang="ts">
	import { resolve } from '$app/paths';
	import PageHeader from '#lib/components/page-header.svelte';
	import EmptyState from '#lib/components/empty-state.svelte';
	import ReadOnlyNote from '#lib/components/read-only-note.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import * as Table from '#lib/components/ui/table/index.js';
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import type { Gym, GymListItem } from '#lib/api/results.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import { splitList } from '#lib/utils.js';

	const gyms = toolQuery<GymListItem[], 'gym_list'>('gym_list', () => ({}));
	const create = toolMutation<Gym, 'gym_create'>('gym_create', {
		invalidate: ['gym_list'],
		success: (g) => `${g.name} added`
	});

	let open = $state(false);
	let name = $state('');
	let aliases = $state('');
	let equipment = $state('');
	let notes = $state('');

	function submit(ev: SubmitEvent) {
		ev.preventDefault();
		create.mutate(
			{
				name: name.trim(),
				aliases: splitList(aliases),
				equipment: equipment.trim() || null,
				notes: notes.trim() || null
			},
			{
				onSuccess: () => {
					open = false;
					name = aliases = equipment = notes = '';
				}
			}
		);
	}

	const when = (t: string | null) =>
		t ? new Date(t).toLocaleDateString(undefined, { dateStyle: 'medium' }) : 'Never';
</script>

<PageHeader title="Gyms" description="Where you train, and what each place has.">
	{#snippet actions()}
		{#if logbook.canWrite('gym')}
			<Button onclick={() => (open = true)}>New gym</Button>
		{/if}
	{/snippet}
</PageHeader>
<ReadOnlyNote domain="gym" />

{#if gyms.isPending}
	<div class="grid gap-2">
		{#each [1, 2, 3] as i (i)}<Skeleton class="h-10 w-full" />{/each}
	</div>
{:else if gyms.isError}
	<p class="text-sm text-destructive">Couldn’t load your gyms: {gyms.error.message}</p>
{:else if gyms.data.length === 0}
	<EmptyState
		title="No gyms yet"
		hint="Each gym you train at shows up here with its last workout. Add one, or tell Claude where you are when you start a workout."
	>
		{#snippet action()}
			{#if logbook.canWrite('gym')}<Button onclick={() => (open = true)}>New gym</Button>{/if}
		{/snippet}
	</EmptyState>
{:else}
	<Table.Root>
		<Table.Header>
			<Table.Row>
				<Table.Head>Name</Table.Head>
				<Table.Head>Other names</Table.Head>
				<Table.Head class="text-right">Last workout</Table.Head>
			</Table.Row>
		</Table.Header>
		<Table.Body>
			{#each gyms.data as g (g.id)}
				<Table.Row>
					<Table.Cell class="font-medium">
						<a class="hover:underline" href={resolve('/gyms/[gym]', { gym: String(g.id) })}
							>{g.name}</a
						>
					</Table.Cell>
					<Table.Cell class="text-muted-foreground">{g.aliases.join(', ')}</Table.Cell>
					<Table.Cell class="text-right tabular-nums">{when(g.last_workout_at)}</Table.Cell>
				</Table.Row>
			{/each}
		</Table.Body>
	</Table.Root>
{/if}

<Dialog.Root bind:open>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>New gym</Dialog.Title>
			<Dialog.Description>Equipment and notes can be added later too.</Dialog.Description>
		</Dialog.Header>
		<form class="grid gap-4" onsubmit={submit}>
			<div class="grid gap-2">
				<Label for="gym-name">Name</Label>
				<Input id="gym-name" required bind:value={name} placeholder="Trainmore NDSM" />
			</div>
			<div class="grid gap-2">
				<Label for="gym-aliases">Other names</Label>
				<Input id="gym-aliases" bind:value={aliases} placeholder="ndsm, trainmore noord" />
				<p class="text-xs text-muted-foreground">Separate with commas.</p>
			</div>
			<div class="grid gap-2">
				<Label for="gym-equipment">Equipment</Label>
				<Textarea
					id="gym-equipment"
					bind:value={equipment}
					placeholder="2 leg presses (max 300 kg), dumbbells 2-40 kg, cable towers. No sled."
				/>
			</div>
			<div class="grid gap-2">
				<Label for="gym-notes">Notes</Label>
				<Textarea id="gym-notes" bind:value={notes} placeholder="Busy after 17:00" />
			</div>
			<Dialog.Footer>
				<Button type="submit" disabled={create.isPending}>Add gym</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
