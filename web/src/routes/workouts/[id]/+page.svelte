<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import PageHeader from '#lib/components/page-header.svelte';
	import ReadOnlyNote from '#lib/components/read-only-note.svelte';
	import ConfirmDelete from '#lib/components/confirm-delete.svelte';
	import EmptyState from '#lib/components/empty-state.svelte';
	import BodyNoteDialog from '#lib/components/body-note-dialog.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import type { BodyNote, GymListItem, SetAddResult, Workout } from '#lib/api/results.js';
	import type { ToolName } from '#lib/api/tools.gen.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import { muscleLabel } from '#lib/muscles.js';
	import {
		BULK_FIELDS,
		SET_TYPES,
		bulkArgs,
		justAfter,
		setTypeLabel,
		workoutForm,
		workoutHref,
		workoutUpdateArgs,
		type BulkField,
		type WorkoutForm
	} from '#lib/workouts.js';
	import SetRow from './set-row.svelte';
	import AddSets from './add-sets.svelte';

	let id = $derived(Number(page.params.id));
	let at = $derived(page.url.searchParams.get('at') ?? '');
	// No workout_get: the workouts started at exactly `at` are this one (and, rarely, one started the same millisecond).
	const found = toolQuery<Workout[], 'workout_list'>(
		'workout_list',
		() => ({ from: at, to: justAfter(at), limit: 50 }),
		{ enabled: () => at !== '' }
	);
	let workout = $derived(found.data?.find((w) => w.workout_id === id));
	let canWrite = $derived(logbook.canWrite('gym'));

	const invalidate: ToolName[] = ['workout_list', 'workout_current'];
	const update = toolMutation<Workout, 'workout_update'>('workout_update', {
		invalidate,
		success: 'Workout saved'
	});
	const finish = toolMutation('workout_finish', { invalidate, success: 'Workout finished' });
	const add = toolMutation<SetAddResult, 'set_add'>('set_add', {
		invalidate,
		success: (r) => (r.added === 1 ? 'Set added' : `${r.added} sets added`)
	});
	const change = toolMutation('set_update', { invalidate, success: 'Set saved' });
	const changeMany = toolMutation('set_update', { invalidate, success: 'Sets changed' });
	const removeMany = toolMutation('set_delete', { invalidate, success: 'Sets deleted' });

	/** A new start moves the workout's address: follow it, so the page keeps finding it. */
	function follow(w: Workout) {
		if (w.started_at !== at) goto(workoutHref(w), { replace: true, reset: false });
	}

	// ---- edit dialog
	const gyms = toolQuery<GymListItem[], 'gym_list'>('gym_list', () => ({}), {
		enabled: () => canWrite
	});
	let form = $state<WorkoutForm | null>(null);
	let startMoved = $derived(
		workout !== undefined && form !== null && form.start !== workoutForm(workout).start
	);

	function openEdit(w: Workout) {
		form = workoutForm(w);
	}

	function saveWorkout(e: SubmitEvent) {
		e.preventDefault();
		if (!workout || !form) return;
		update.mutate(workoutUpdateArgs(workout, form), {
			onSuccess: (w) => {
				form = null;
				follow(w);
			}
		});
	}

	// ---- bulk fixes
	let selected = $state<Record<number, boolean>>({});
	let selectedIds = $derived(
		Object.entries(selected)
			.filter(([, on]) => on)
			.map(([k]) => Number(k))
	);
	let bulkField = $state<BulkField>('set_type');
	let bulkValue = $state('');
	let bulkArgsNow = $derived(bulkArgs(selectedIds, bulkField, bulkValue));

	function changeSelected() {
		if (bulkArgsNow) changeMany.mutate(bulkArgsNow, { onSuccess: () => (bulkValue = '') });
	}

	// ---- body notes
	// shortcut: body_note_list has no workout filter, so this loads every note and keeps this workout's; add one to the server if lists grow long.
	const notes = toolQuery<BodyNote[], 'body_note_list'>('body_note_list', () => ({}));
	let workoutNotes = $derived(notes.data?.filter((n) => n.workout_id === id) ?? []);

	const day = (iso: string) =>
		new Date(iso).toLocaleDateString(undefined, {
			weekday: 'long',
			day: 'numeric',
			month: 'long',
			year: 'numeric'
		});
	const clock = (iso: string) =>
		new Date(iso).toLocaleTimeString(undefined, { timeStyle: 'short' });
	const when = (iso: string) =>
		new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
</script>

<a
	href={resolve('/workouts')}
	class="mb-2 inline-block text-sm text-muted-foreground hover:underline">← All workouts</a
>
{#if at && found.isPending}
	<Skeleton class="mb-4 h-8 w-64" />
	<Skeleton class="h-48 w-full" />
{:else if found.isError}
	<p class="text-sm text-destructive">This workout could not be loaded: {found.error.message}</p>
{:else if !workout}
	<p class="text-sm text-muted-foreground">
		This workout can’t be found: it may have been deleted, or moved to another time. Open it from
		the <a class="underline" href={resolve('/workouts')}>workouts list</a>.
	</p>
{:else}
	{@const w = workout}
	<PageHeader
		title={day(w.started_at)}
		description="{w.gym.name} · {clock(w.started_at)}–{w.finished_at
			? clock(w.finished_at)
			: 'now'} · {w.set_count} {w.set_count === 1 ? 'set' : 'sets'}"
	>
		{#snippet actions()}
			{#if canWrite}
				<Button variant="outline" onclick={() => openEdit(w)}>Edit workout</Button>
				{#if !w.finished_at}
					<Button onclick={() => finish.mutate({})} disabled={finish.isPending}
						>Finish workout</Button
					>
				{/if}
			{/if}
		{/snippet}
	</PageHeader>
	<div class="-mt-4 mb-4 flex flex-wrap gap-2">
		{#if !w.finished_at}<Badge>In progress</Badge>{/if}
		{#if w.time_estimated}<Badge variant="outline">Times are estimates</Badge>{/if}
		{#if w.finish_reason === 'stale'}<Badge variant="outline">Finished automatically</Badge>{/if}
		{#if w.finish_reason === 'superseded'}<Badge variant="outline"
				>Finished when the next one started</Badge
			>{/if}
	</div>
	<ReadOnlyNote domain="gym" />
	{#if w.notes}<p class="mb-6 max-w-2xl text-sm whitespace-pre-line">{w.notes}</p>{/if}

	<div class="grid max-w-4xl gap-8">
		{#if canWrite && selectedIds.length}
			<section
				class="sticky top-2 z-10 flex flex-wrap items-end gap-3 rounded-lg border bg-background p-3 shadow-sm"
				aria-label="Change selected sets"
			>
				<p class="text-sm font-medium tabular-nums">
					{selectedIds.length} selected
				</p>
				<div class="grid gap-1.5">
					<Label for="bulk-field">Field</Label>
					<Select.Root
						type="single"
						value={bulkField}
						onValueChange={(v) => {
							bulkField = v as BulkField;
							bulkValue = '';
						}}
					>
						<Select.Trigger id="bulk-field" class="w-36"
							>{BULK_FIELDS.find((f) => f.field === bulkField)?.label}</Select.Trigger
						>
						<Select.Content>
							{#each BULK_FIELDS as f (f.field)}<Select.Item
									value={f.field}
									label={f.label}
								/>{/each}
						</Select.Content>
					</Select.Root>
				</div>
				<div class="grid gap-1.5">
					<Label for="bulk-value">New value</Label>
					{#if bulkField === 'set_type'}
						<Select.Root type="single" bind:value={bulkValue}>
							<Select.Trigger id="bulk-value" class="w-36"
								>{bulkValue ? setTypeLabel(bulkValue as never) : 'Choose'}</Select.Trigger
							>
							<Select.Content>
								{#each SET_TYPES as t (t.value)}<Select.Item
										value={t.value}
										label={t.label}
									/>{/each}
							</Select.Content>
						</Select.Root>
					{:else}
						<Input id="bulk-value" class="w-36" bind:value={bulkValue} />
					{/if}
				</div>
				<Button
					onclick={changeSelected}
					disabled={!bulkArgsNow ||
						(bulkField === 'set_type' && !bulkValue) ||
						changeMany.isPending}>Change selected</Button
				>
				<ConfirmDelete
					label="Delete selected"
					description="{selectedIds.length} {selectedIds.length === 1
						? 'set is'
						: 'sets are'} deleted for good, with any comments on them."
					onconfirm={() =>
						removeMany.mutate({ set_ids: selectedIds }, { onSuccess: () => (selected = {}) })}
				/>
				<Button variant="ghost" onclick={() => (selected = {})}>Clear selection</Button>
			</section>
		{/if}

		{#if !w.exercises.length}
			<EmptyState
				title="No sets yet"
				hint={canWrite
					? 'Sets you add below, or tell Claude, show up here grouped by exercise.'
					: 'Sets show up here, grouped by exercise, once they are logged.'}
			/>
		{:else}
			{#each w.exercises as ex (ex.exercise_id)}
				<section class="grid gap-1" aria-labelledby="ex-{ex.exercise_id}">
					<h2 id="ex-{ex.exercise_id}" class="font-semibold">
						<a
							class="hover:underline"
							href={resolve('/exercises/[id]', { id: String(ex.exercise_id) })}>{ex.exercise}</a
						>
					</h2>
					<ul class="divide-y">
						{#each ex.sets as s (s.set_id)}
							<SetRow
								set={s}
								editable={canWrite}
								bind:selected={() => selected[s.set_id] ?? false, (v) => (selected[s.set_id] = v)}
								onsave={(args) => change.mutateAsync(args)}
							/>
						{/each}
					</ul>
				</section>
			{/each}
		{/if}

		{#if canWrite}
			<AddSets workout={w} onadd={(args) => add.mutateAsync(args).then((r) => follow(r.workout))} />
		{/if}

		<section class="grid gap-3" aria-labelledby="notes-title">
			<div class="flex flex-wrap items-center justify-between gap-2">
				<h2 id="notes-title" class="font-semibold">Body notes</h2>
				{#if canWrite}<BodyNoteDialog workoutId={w.workout_id} />{/if}
			</div>
			{#if notes.isPending}
				<Skeleton class="h-10 w-full" />
			{:else if notes.isError}
				<p class="text-sm text-destructive">
					Body notes could not be loaded: {notes.error.message}
				</p>
			{:else if !workoutNotes.length}
				<p class="text-sm text-muted-foreground">
					No body notes for this workout. An ache or pain that came up during it goes here.
				</p>
			{:else}
				<ul class="divide-y rounded-lg border">
					{#each workoutNotes as n (n.id)}
						<li class="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2 text-sm">
							<span class="font-medium">{muscleLabel(n.location)}</span>
							{#if n.severity !== null}<span class="tabular-nums">{n.severity}/10</span>{/if}
							<span class="text-muted-foreground tabular-nums">{when(n.noted_at)}</span>
							{#if n.notes}<span class="text-muted-foreground">{n.notes}</span>{/if}
							{#if canWrite}<span class="ml-auto"><BodyNoteDialog note={n} /></span>{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	</div>
{/if}

<Dialog.Root open={form !== null} onOpenChange={(o) => !o && (form = null)}>
	<Dialog.Content>
		{#if form}
			<Dialog.Header>
				<Dialog.Title>Edit workout</Dialog.Title>
				<Dialog.Description>When and where it was, in your own timezone.</Dialog.Description>
			</Dialog.Header>
			<form class="grid gap-4" onsubmit={saveWorkout}>
				<div class="grid gap-3 sm:grid-cols-2">
					<div class="grid gap-2">
						<Label for="w-start">Start</Label>
						<Input id="w-start" type="datetime-local" required bind:value={form.start} />
					</div>
					{#if workout?.finished_at}
						<div class="grid gap-2">
							<Label for="w-finish">Finish</Label>
							<Input id="w-finish" type="datetime-local" required bind:value={form.finish} />
						</div>
					{/if}
				</div>
				{#if startMoved}
					<div class="flex items-center gap-2">
						<Checkbox id="w-shift" bind:checked={form.shift_sets} />
						<Label for="w-shift" class="font-normal"
							>Move the sets along (and the finish), keeping their spacing</Label
						>
					</div>
				{/if}
				<div class="grid gap-2">
					<Label for="w-gym">Gym</Label>
					<Select.Root type="single" bind:value={form.gym}>
						<Select.Trigger id="w-gym" class="w-full"
							>{gyms.data?.find((g) => String(g.id) === form?.gym)?.name ??
								workout?.gym.name}</Select.Trigger
						>
						<Select.Content>
							{#each gyms.data ?? [] as g (g.id)}<Select.Item
									value={String(g.id)}
									label={g.name}
								/>{/each}
						</Select.Content>
					</Select.Root>
				</div>
				<div class="grid gap-2">
					<Label for="w-notes">Notes</Label>
					<Textarea id="w-notes" bind:value={form.notes} />
				</div>
				<div class="flex items-center gap-2">
					<Checkbox id="w-estimated" bind:checked={form.time_estimated} />
					<Label for="w-estimated" class="font-normal">The times are estimates</Label>
				</div>
				<Dialog.Footer>
					<Button type="submit" disabled={update.isPending}>Save workout</Button>
				</Dialog.Footer>
			</form>
		{/if}
	</Dialog.Content>
</Dialog.Root>
