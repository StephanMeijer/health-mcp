<script lang="ts">
	import * as Table from '#lib/components/ui/table/index.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import ConfirmDelete from '#lib/components/confirm-delete.svelte';
	import type { KindInfo } from '#lib/kinds.js';
	import type { ReadingRow } from '#lib/api/results.js';
	import { SOURCE_LABEL, dateTime, num, readingValue } from '#lib/vitals.js';

	/** Readings of one kind. Only manual rows can be changed: the tools leave app and imported readings alone. */
	let {
		kind,
		rows,
		canWrite,
		onedit,
		ondelete
	}: {
		kind: KindInfo;
		rows: ReadingRow[];
		canWrite: boolean;
		onedit: (row: ReadingRow) => void;
		ondelete: (row: ReadingRow) => void;
	} = $props();
</script>

<Table.Root>
	<Table.Header>
		<Table.Row>
			<Table.Head>Measured</Table.Head>
			<Table.Head class="text-right">{kind.label}</Table.Head>
			<Table.Head>Source</Table.Head>
			<Table.Head>Note</Table.Head>
			{#if canWrite}<Table.Head><span class="sr-only">Actions</span></Table.Head>{/if}
		</Table.Row>
	</Table.Header>
	<Table.Body>
		{#each rows as row (`${row.source}:${'device_id' in row ? row.device_id : ''}:${row.time}`)}
			<Table.Row>
				<Table.Cell class="tabular-nums">{dateTime(row.time)}</Table.Cell>
				<Table.Cell class="text-right tabular-nums">
					{num(readingValue(row, kind), 2)} <span class="text-muted-foreground">{kind.unit}</span>
				</Table.Cell>
				<Table.Cell>
					<Badge variant={row.source === 'manual' ? 'secondary' : 'outline'}>
						{SOURCE_LABEL[row.source]}
					</Badge>
				</Table.Cell>
				<Table.Cell class="max-w-xs truncate text-muted-foreground">{row.note ?? ''}</Table.Cell>
				{#if canWrite}
					<Table.Cell class="text-right">
						{#if row.source === 'manual'}
							<div class="flex justify-end gap-1">
								<Button variant="ghost" size="sm" onclick={() => onedit(row)}>Edit</Button>
								<ConfirmDelete
									label="Delete reading"
									description="The {kind.label.toLowerCase()} reading of {dateTime(
										row.time
									)} is removed for good."
									onconfirm={() => ondelete(row)}
								/>
							</div>
						{:else}
							<span class="text-xs text-muted-foreground" title="Change it where it came from">
								Read-only
							</span>
						{/if}
					</Table.Cell>
				{/if}
			</Table.Row>
		{/each}
	</Table.Body>
</Table.Root>
