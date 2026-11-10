<script lang="ts">
	import { resolve } from '$app/paths';
	import * as Select from '#lib/components/ui/select/index.js';
	import * as Table from '#lib/components/ui/table/index.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import PageHeader from '#lib/components/page-header.svelte';
	import EmptyState from '#lib/components/empty-state.svelte';
	import ReadOnlyNote from '#lib/components/read-only-note.svelte';
	import ConfirmDelete from '#lib/components/confirm-delete.svelte';
	import SleepForm from '#lib/components/vitals/sleep-form.svelte';
	import StageBar from '#lib/components/vitals/stage-bar.svelte';
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import { sessions, type SleepList, type SleepSession } from '#lib/api/results.js';
	import type { SleepAddArgs } from '#lib/api/tools.gen.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import { RANGES, SOURCE_LABEL, daysAgo, num } from '#lib/vitals.js';

	let days = $state('30');
	let from = $derived(daysAgo(Number(days)));
	let canWrite = $derived(logbook.canWrite('vitals'));

	const list = toolQuery<unknown, 'sleep_list'>('sleep_list', () => ({
		from,
		include_stages: false
	}));
	let nights = $derived(sessions(list.data).toReversed());
	// sleep_list keeps the oldest 1000 sessions of the period and says so with `truncated`.
	let truncated = $derived((list.data as SleepList | undefined)?.truncated === true);

	const invalidate: ['sleep_list', 'summary'] = ['sleep_list', 'summary'];
	const add = toolMutation('sleep_add', { invalidate, success: 'Sleep logged' });
	const update = toolMutation('sleep_update', { invalidate, success: 'Sleep updated' });
	const remove = toolMutation('sleep_delete', { invalidate, success: 'Sleep deleted' });

	let form = $state<{ session: SleepSession | null } | null>(null);

	async function onsave(args: SleepAddArgs) {
		const s = form?.session;
		try {
			if (s)
				await update.mutateAsync({
					...args,
					id: s.id,
					stages: args.stages?.length ? args.stages : undefined
				});
			else await add.mutateAsync(args);
			form = null;
		} catch {
			// toolMutation showed the server's message; the form stays open to fix it.
		}
	}

	const day = (iso: string) =>
		new Date(iso).toLocaleDateString(undefined, {
			weekday: 'short',
			day: 'numeric',
			month: 'short'
		});
	const clock = (iso: string) =>
		new Date(iso).toLocaleTimeString(undefined, { timeStyle: 'short' });
</script>

<a href={resolve('/vitals')} class="text-sm text-muted-foreground hover:underline">← Vitals</a>
<PageHeader title="Sleep" description="Each night with its hours and how it split into stages.">
	{#snippet actions()}
		{#if canWrite}<Button onclick={() => (form = { session: null })}>Log sleep</Button>{/if}
	{/snippet}
</PageHeader>
<ReadOnlyNote domain="vitals" />

<div class="mb-4 grid gap-1.5">
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

{#if list.isPending}
	<div class="grid gap-2">
		{#each { length: 5 }, i (i)}<Skeleton class="h-10" />{/each}
	</div>
{:else if list.isError}
	<p class="text-sm text-destructive">Your sleep could not be loaded: {list.error.message}</p>
{:else if !nights.length}
	<EmptyState
		title="No nights in this period"
		hint={canWrite
			? 'Nights you log here, or that an app syncs, show up in this list. Try a longer period, or log last night.'
			: 'Nights show up here once they are logged. Try a longer period.'}
	/>
{:else}
	<Table.Root>
		<Table.Header>
			<Table.Row>
				<Table.Head>Night</Table.Head>
				<Table.Head>Asleep</Table.Head>
				<Table.Head class="text-right">Hours</Table.Head>
				<Table.Head class="w-1/4">Stages</Table.Head>
				<Table.Head>Source</Table.Head>
				<Table.Head>Note</Table.Head>
				{#if canWrite}<Table.Head><span class="sr-only">Actions</span></Table.Head>{/if}
			</Table.Row>
		</Table.Header>
		<Table.Body>
			{#each nights as s (s.id)}
				<Table.Row>
					<Table.Cell>{day(s.start_at)}</Table.Cell>
					<Table.Cell class="tabular-nums">{clock(s.start_at)} – {clock(s.end_at)}</Table.Cell>
					<Table.Cell class="text-right tabular-nums">{num(s.hours)} h</Table.Cell>
					<Table.Cell><StageBar split={s.stage_seconds} /></Table.Cell>
					<Table.Cell>
						<Badge variant={s.source === 'manual' ? 'secondary' : 'outline'}
							>{SOURCE_LABEL[s.source]}</Badge
						>
					</Table.Cell>
					<Table.Cell class="max-w-xs truncate text-muted-foreground">{s.note ?? ''}</Table.Cell>
					{#if canWrite}
						<Table.Cell class="text-right">
							{#if s.source === 'manual'}
								<div class="flex justify-end gap-1">
									<Button variant="ghost" size="sm" onclick={() => (form = { session: s })}
										>Edit</Button
									>
									<ConfirmDelete
										label="Delete sleep"
										description="The night of {day(
											s.start_at
										)} and its stages are removed for good."
										onconfirm={() => remove.mutate({ id: s.id })}
									/>
								</div>
							{:else}
								<span class="text-xs text-muted-foreground" title="Change it where it came from"
									>Read-only</span
								>
							{/if}
						</Table.Cell>
					{/if}
				</Table.Row>
			{/each}
		</Table.Body>
	</Table.Root>
	{#if truncated}
		<p class="mt-2 text-xs text-muted-foreground">
			Showing the oldest 1000 nights of this period; choose a shorter period to see newer ones.
		</p>
	{/if}
{/if}

{#if form}
	<SleepForm session={form.session} {onsave} onclose={() => (form = null)} />
{/if}
