<script lang="ts">
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import type { BodyNote } from '#lib/api/results.js';
	import ConfirmDelete from '#lib/components/confirm-delete.svelte';
	import EmptyState from '#lib/components/empty-state.svelte';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Table from '#lib/components/ui/table/index.js';
	import { muscleLabel } from '#lib/muscles.js';
	import BodyNoteDialog from './body-note-dialog.svelte';

	let { readonly = false }: { readonly?: boolean } = $props();

	let location = $state('');
	let from = $state('');
	let to = $state('');

	const notes = toolQuery<BodyNote[]>('body_note_list', () => ({
		...(location.trim() && { location: location.trim() }),
		...(from && { from }),
		...(to && { to })
	}));
	const filtered = $derived(Boolean(location.trim() || from || to));

	const del = toolMutation('body_note_delete', {
		invalidate: ['body_note_list', 'training_overview'],
		success: 'Body note deleted'
	});

	const when = (iso: string) =>
		new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
</script>

<div class="mb-4 flex flex-wrap items-end gap-3">
	<div class="grid gap-1.5">
		<Label for="filter-location">Where</Label>
		<!-- Filters on Enter or leaving the field, not on every key. -->
		<Input
			id="filter-location"
			value={location}
			onchange={(e) => (location = e.currentTarget.value)}
			placeholder="knee, back"
			class="w-44"
		/>
	</div>
	<div class="grid gap-1.5">
		<Label for="filter-from">From</Label>
		<Input id="filter-from" type="date" bind:value={from} class="w-40" />
	</div>
	<div class="grid gap-1.5">
		<Label for="filter-to">Before</Label>
		<Input id="filter-to" type="date" bind:value={to} class="w-40" />
	</div>
	{#if !readonly}<div class="ml-auto"><BodyNoteDialog /></div>{/if}
</div>

{#if notes.isPending}
	<div class="grid gap-2">
		{#each [1, 2, 3] as i (i)}<Skeleton class="h-10 w-full" />{/each}
	</div>
{:else if notes.isError}
	<p class="text-sm text-destructive">
		Body notes could not be loaded: {notes.error.message}
	</p>
{:else if notes.data.length === 0}
	<EmptyState
		title={filtered ? 'No body notes match' : 'No body notes yet'}
		hint={filtered
			? 'Change or clear the filters to see more.'
			: 'Aches, pains and fatigue you note appear here, so workouts can work around them. Add one, or tell Claude.'}
	/>
{:else}
	<Table.Root>
		<Table.Header>
			<Table.Row>
				<Table.Head>When</Table.Head>
				<Table.Head>Where</Table.Head>
				<Table.Head class="text-right">How bad</Table.Head>
				<Table.Head>Notes</Table.Head>
				<Table.Head>Workout</Table.Head>
				{#if !readonly}<Table.Head><span class="sr-only">Actions</span></Table.Head>{/if}
			</Table.Row>
		</Table.Header>
		<Table.Body>
			{#each notes.data as n (n.id)}
				<Table.Row>
					<Table.Cell class="whitespace-nowrap tabular-nums">{when(n.noted_at)}</Table.Cell>
					<Table.Cell>{muscleLabel(n.location)}</Table.Cell>
					<Table.Cell class="text-right tabular-nums">
						{n.severity === null ? '–' : `${n.severity}/10`}
					</Table.Cell>
					<Table.Cell class="max-w-xs whitespace-normal text-muted-foreground">{n.notes}</Table.Cell
					>
					<Table.Cell class="tabular-nums">
						<!-- shortcut: a plain id; link it once workouts have pages (#104). -->
						{n.workout_id === null ? '' : `#${n.workout_id}`}
					</Table.Cell>
					{#if !readonly}
						<Table.Cell class="text-right whitespace-nowrap">
							<BodyNoteDialog note={n} />
							<ConfirmDelete
								label="Delete body note"
								description="The note about {n.location} from {when(
									n.noted_at
								)} is deleted for good."
								onconfirm={() => del.mutate({ id: n.id })}
							/>
						</Table.Cell>
					{/if}
				</Table.Row>
			{/each}
		</Table.Body>
	</Table.Root>
{/if}
