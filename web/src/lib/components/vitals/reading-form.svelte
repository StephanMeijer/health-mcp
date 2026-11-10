<script lang="ts">
	import { untrack } from 'svelte';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import type { KindInfo } from '#lib/kinds.js';
	import type { ReadingRow } from '#lib/api/results.js';
	import type { Kind, Values } from '#lib/api/tools.gen.js';
	import {
		type AddCall,
		dateTime,
		fromLocalInput,
		isBloodPressure,
		readingValue,
		toLocalInput
	} from '#lib/vitals.js';

	/**
	 * Add a reading of `kind`, or edit `row` (value and note; the time identifies it). Adding systolic or diastolic
	 * logs a whole blood pressure measurement: one reading each, plus pulse when given, all at the same time.
	 */
	let {
		kind,
		row = null,
		onadd,
		onupdate,
		onclose
	}: {
		kind: KindInfo;
		row?: ReadingRow | null;
		onadd: (calls: AddCall[]) => Promise<void>;
		onupdate: (time: string, set: Values) => Promise<void>;
		onclose: () => void;
	} = $props();

	const initial = untrack(() => ({ kind, row }));
	const bp = !initial.row && isBloodPressure(initial.kind.kind);
	let time = $state(toLocalInput(initial.row?.time ?? new Date().toISOString()));
	let value = $state<number | null>(initial.row ? readingValue(initial.row, initial.kind) : null);
	let systolic = $state<number | null>(null);
	let diastolic = $state<number | null>(null);
	let pulse = $state<number | null>(null);
	let note = $state(initial.row?.note ?? '');
	let busy = $state(false);

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		const n = note.trim() || null;
		busy = true;
		try {
			if (row) {
				// '' clears the note: the update skips a null field.
				await onupdate(row.time, { [kind.field]: value, note: note.trim() });
			} else if (bp) {
				const t = fromLocalInput(time);
				const one = (k: Kind, v: number | null): AddCall => ({
					kind: k,
					readings: [{ time: t, value: v, note: n }]
				});
				await onadd([
					one('systolic', systolic),
					one('diastolic', diastolic),
					...(pulse == null ? [] : [one('pulse', pulse)])
				]);
			} else {
				await onadd([
					{
						kind: kind.kind as Kind,
						readings: [{ time: fromLocalInput(time), [kind.field]: value, note: n }]
					}
				]);
			}
		} finally {
			busy = false;
		}
	}
</script>

<Dialog.Root open onOpenChange={(o) => !o && onclose()}>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>
				{row
					? `Edit ${kind.label.toLowerCase()}`
					: bp
						? 'Add blood pressure'
						: `Add ${kind.label.toLowerCase()}`}
			</Dialog.Title>
			<Dialog.Description>
				{#if row}
					Measured {dateTime(row.time)}. To change the time, delete it and add it again.
				{:else if bp}
					Systolic and diastolic in mmHg, pulse in bpm if you have it. They are saved at the same
					time.
				{:else}
					In {kind.unit}, at the time you measured it.
				{/if}
			</Dialog.Description>
		</Dialog.Header>
		<form class="grid gap-4" onsubmit={submit}>
			{#if !row}
				<div class="grid gap-2">
					<Label for="reading-time">Measured at</Label>
					<Input id="reading-time" type="datetime-local" bind:value={time} required />
				</div>
			{/if}
			{#if bp}
				<div class="grid grid-cols-3 gap-3">
					<div class="grid gap-2">
						<Label for="reading-systolic">Systolic (mmHg)</Label>
						<Input
							id="reading-systolic"
							type="number"
							step="any"
							min="0"
							bind:value={systolic}
							required
						/>
					</div>
					<div class="grid gap-2">
						<Label for="reading-diastolic">Diastolic (mmHg)</Label>
						<Input
							id="reading-diastolic"
							type="number"
							step="any"
							min="0"
							bind:value={diastolic}
							required
						/>
					</div>
					<div class="grid gap-2">
						<Label for="reading-pulse">Pulse (bpm)</Label>
						<Input id="reading-pulse" type="number" step="any" min="0" bind:value={pulse} />
					</div>
				</div>
			{:else}
				<div class="grid gap-2">
					<Label for="reading-value">{kind.label} ({kind.unit})</Label>
					<Input
						id="reading-value"
						type="number"
						step={kind.field === 'bpm' ? 1 : 'any'}
						bind:value
						required
					/>
				</div>
			{/if}
			<div class="grid gap-2">
				<Label for="reading-note">Note (optional)</Label>
				<Input id="reading-note" bind:value={note} placeholder="After a run, before coffee…" />
			</div>
			<Dialog.Footer>
				<Button type="button" variant="outline" onclick={onclose}>Cancel</Button>
				<Button type="submit" disabled={busy}>
					{row ? 'Save reading' : bp ? 'Add blood pressure' : 'Add reading'}
				</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
