<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { toast } from 'svelte-sonner';
	import * as Select from '#lib/components/ui/select/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import PageHeader from '#lib/components/page-header.svelte';
	import EmptyState from '#lib/components/empty-state.svelte';
	import ReadOnlyNote from '#lib/components/read-only-note.svelte';
	import ReadingForm from '#lib/components/vitals/reading-form.svelte';
	import ReadingTable from '#lib/components/vitals/reading-table.svelte';
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import type { ReadingRow, Source } from '#lib/api/results.js';
	import type { Kind } from '#lib/api/tools.gen.js';
	import { kindInfo } from '#lib/kinds.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import {
		RANGES,
		SOURCE_LABEL,
		addAll,
		daysAgo,
		isBloodPressure,
		type AddCall
	} from '#lib/vitals.js';

	const LIMIT = 1000;
	const ALL = 'all';

	let info = $derived(kindInfo(page.params.kind ?? ''));
	let kind = $derived(info?.kind as Kind);
	let days = $state('30');
	let source = $state<string>(ALL);
	let from = $derived(daysAgo(Number(days)));
	let canWrite = $derived(logbook.canWrite('vitals'));

	const list = toolQuery<ReadingRow[], 'reading_list'>(
		'reading_list',
		() => ({ kind, from, limit: LIMIT, source: source === ALL ? null : (source as Source) }),
		{ enabled: () => !!info }
	);
	const invalidate = ['reading_list', 'summary'] as const;
	const add = toolMutation('reading_add', { invalidate: [...invalidate] });
	const update = toolMutation('reading_update', {
		invalidate: [...invalidate],
		success: 'Reading updated'
	});
	const remove = toolMutation('reading_delete', {
		invalidate: [...invalidate],
		success: 'Reading deleted'
	});
	const undo = toolMutation('reading_delete', { invalidate: [...invalidate] });

	let form = $state<{ row: ReadingRow | null } | null>(null);

	async function onadd(calls: AddCall[]) {
		try {
			await addAll(calls, add.mutateAsync, undo.mutateAsync);
		} catch {
			return; // toolMutation already said what went wrong; keep the form open to fix it.
		}
		toast.success(calls.length > 1 ? 'Blood pressure added' : 'Reading added');
		form = null;
	}

	async function onupdate(time: string, set: Parameters<typeof update.mutateAsync>[0]['set']) {
		try {
			await update.mutateAsync({ kind, time, set });
			form = null;
		} catch {
			// toast shown by toolMutation
		}
	}
</script>

{#if !info}
	<PageHeader title="Unknown vital" />
	<p class="text-sm text-muted-foreground">
		There is no vital called “{page.params.kind}”.
		<a class="underline" href={resolve('/vitals')}>Back to vitals</a>
	</p>
{:else}
	<a href={resolve('/vitals')} class="text-sm text-muted-foreground hover:underline">← Vitals</a>
	<PageHeader
		title={info.label}
		description="Every reading in {info.unit}, newest first, in your timezone."
	>
		{#snippet actions()}
			{#if canWrite}
				<Button onclick={() => (form = { row: null })}>
					{isBloodPressure(info.kind) ? 'Add blood pressure' : 'Add reading'}
				</Button>
			{/if}
		{/snippet}
	</PageHeader>
	<ReadOnlyNote domain="vitals" />

	<div class="mb-4 flex flex-wrap gap-4">
		<div class="grid gap-1.5">
			<Label for="range">Period</Label>
			<Select.Root type="single" bind:value={days}>
				<Select.Trigger id="range" class="w-40"
					>{RANGES.find((r) => r.value === days)?.label}</Select.Trigger
				>
				<Select.Content>
					{#each RANGES as r (r.value)}<Select.Item value={r.value} label={r.label}
							>{r.label}</Select.Item
						>{/each}
				</Select.Content>
			</Select.Root>
		</div>
		<div class="grid gap-1.5">
			<Label for="source">Source</Label>
			<Select.Root type="single" bind:value={source}>
				<Select.Trigger id="source" class="w-44">
					{source === ALL ? 'All sources' : SOURCE_LABEL[source as Source]}
				</Select.Trigger>
				<Select.Content>
					<Select.Item value={ALL} label="All sources">All sources</Select.Item>
					{#each Object.entries(SOURCE_LABEL) as [value, label] (value)}
						<Select.Item {value} {label}>{label}</Select.Item>
					{/each}
				</Select.Content>
			</Select.Root>
		</div>
	</div>

	{#if list.isPending}
		<div class="grid gap-2">
			{#each { length: 5 }, i (i)}<Skeleton class="h-10" />{/each}
		</div>
	{:else if list.isError}
		<p class="text-sm text-destructive">The readings could not be loaded: {list.error.message}</p>
	{:else if !list.data.length}
		<EmptyState
			title="No {info.label.toLowerCase()} readings in this period"
			hint={canWrite
				? 'Readings you add here, or that an app syncs, show up in this list. Try a longer period, or add one now.'
				: 'Readings show up here once they are logged. Try a longer period.'}
		/>
	{:else}
		<ReadingTable
			kind={info}
			rows={list.data}
			{canWrite}
			onedit={(row) => (form = { row })}
			ondelete={(row) => remove.mutate({ kind, times: [row.time] })}
		/>
		{#if list.data.length === LIMIT}
			<p class="mt-2 text-xs text-muted-foreground">
				Showing the newest {LIMIT} readings of this period.
			</p>
		{/if}
	{/if}

	{#if form}
		<ReadingForm kind={info} row={form.row} {onadd} {onupdate} onclose={() => (form = null)} />
	{/if}
{/if}
