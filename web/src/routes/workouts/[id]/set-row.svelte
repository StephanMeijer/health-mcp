<script lang="ts">
	import { untrack } from 'svelte';
	import MessageSquareIcon from '@lucide/svelte/icons/message-square';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import type { WorkoutSet } from '#lib/api/results.js';
	import type { SetUpdateArgs } from '#lib/api/tools.gen.js';
	import { plateClass, plateFor } from '#lib/plates.js';
	import {
		LOAD_BASES,
		SET_TYPES,
		setForm,
		setSummary,
		setTypeLabel,
		setUpdateArgs
	} from '#lib/workouts.js';

	/**
	 * One set: what it was, and (when `editable`) a checkbox for bulk fixes and an inline editor that saves only the
	 * fields that changed through `onsave`. `comments` is how many comments its threads have; `oncomments` opens them.
	 */
	let {
		set,
		editable,
		selected = $bindable(false),
		onsave,
		comments = 0,
		oncomments
	}: {
		set: WorkoutSet;
		editable: boolean;
		selected?: boolean;
		onsave: (args: Omit<SetUpdateArgs, 'owner'>) => Promise<unknown>;
		comments?: number;
		oncomments?: () => void;
	} = $props();

	let editing = $state(false);
	let more = $state(false);
	let busy = $state(false);
	let form = $state(setForm(untrack(() => set)));
	const id = $derived(`set-${set.set_id}`);

	function edit() {
		form = setForm(set);
		more = set.duration_s != null || set.distance_m != null;
		editing = true;
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		const args = setUpdateArgs(set, form);
		if (Object.keys(args).length === 1) {
			editing = false;
			return;
		}
		busy = true;
		try {
			await onsave(args);
			editing = false;
		} catch {
			// The toast says why; the editor stays open to fix it.
		} finally {
			busy = false;
		}
	}

	const time = (iso: string) => new Date(iso).toLocaleTimeString(undefined, { timeStyle: 'short' });
	let details = $derived(
		[
			set.rpe != null && `RPE ${set.rpe}`,
			set.rest_s != null && `rest ${set.rest_s} s`,
			set.avg_hr != null && `${set.avg_hr} bpm`,
			set.calories != null && `${set.calories} kcal`,
			set.machine,
			set.variant
		].filter(Boolean)
	);
</script>

<li id="set-{set.set_id}" class="grid scroll-mt-20 gap-1 py-2">
	<div class="flex flex-wrap items-center gap-x-3 gap-y-1">
		{#if editable}
			<Checkbox
				bind:checked={selected}
				aria-label="Select set {set.set_number}"
				class="after:inset-0"
			/>
		{/if}
		<span class="w-5 text-sm text-muted-foreground tabular-nums">{set.set_number}</span>
		<span
			class="size-3 shrink-0 rounded-full ring-1 ring-foreground/10 {plateClass(
				plateFor(set.weight_kg)
			)}"
			aria-hidden="true"
		></span>
		<span class="font-medium tabular-nums">{setSummary(set)}</span>
		{#if set.set_type !== 'working'}<Badge variant="outline">{setTypeLabel(set.set_type)}</Badge
			>{/if}
		{#if details.length}
			<span class="text-sm text-muted-foreground tabular-nums">{details.join(' · ')}</span>
		{/if}
		<span class="ml-auto text-xs text-muted-foreground tabular-nums">
			{set.time_estimated ? '~' : ''}{time(set.performed_at)}
		</span>
		{#if oncomments}
			<Button
				variant="ghost"
				size="sm"
				class="tabular-nums"
				aria-label={comments
					? `${comments} ${comments === 1 ? 'comment' : 'comments'} on set ${set.set_number}`
					: `Comment on set ${set.set_number}`}
				onclick={oncomments}><MessageSquareIcon aria-hidden="true" />{comments || ''}</Button
			>
		{/if}
		{#if editable}
			<Button
				variant="ghost"
				size="sm"
				aria-expanded={editing}
				onclick={() => (editing ? (editing = false) : edit())}>{editing ? 'Close' : 'Edit'}</Button
			>
		{/if}
	</div>
	{#if set.notes}<p class="pl-8 text-sm text-muted-foreground">{set.notes}</p>{/if}

	{#if editing}
		<form class="mt-2 grid gap-4 rounded-md border p-3" onsubmit={submit}>
			<div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
				<div class="grid gap-1.5">
					<Label for="{id}-reps">Reps</Label>
					<Input
						id="{id}-reps"
						type="number"
						min="0"
						step="1"
						class="tabular-nums"
						bind:value={form.reps}
					/>
				</div>
				<div class="grid gap-1.5">
					<Label for="{id}-weight">Weight (kg)</Label>
					<Input
						id="{id}-weight"
						type="number"
						min="0"
						step="any"
						class="tabular-nums"
						bind:value={form.weight_kg}
					/>
				</div>
				<div class="grid gap-1.5">
					<Label for="{id}-rpe">RPE (1–10)</Label>
					<Input
						id="{id}-rpe"
						type="number"
						min="1"
						max="10"
						step="0.5"
						class="tabular-nums"
						bind:value={form.rpe}
					/>
				</div>
				<div class="grid gap-1.5">
					<Label for="{id}-rest">Rest before (s)</Label>
					<Input
						id="{id}-rest"
						type="number"
						min="0"
						step="1"
						class="tabular-nums"
						bind:value={form.rest_s}
					/>
				</div>
				<div class="grid gap-1.5">
					<Label for="{id}-type">Set type</Label>
					<Select.Root type="single" bind:value={form.set_type}>
						<Select.Trigger id="{id}-type" class="w-full"
							>{setTypeLabel(form.set_type)}</Select.Trigger
						>
						<Select.Content>
							{#each SET_TYPES as t (t.value)}<Select.Item value={t.value} label={t.label} />{/each}
						</Select.Content>
					</Select.Root>
				</div>
				<div class="grid gap-1.5">
					<Label for="{id}-machine">Machine</Label>
					<Input id="{id}-machine" bind:value={form.machine} />
				</div>
				<div class="grid gap-1.5">
					<Label for="{id}-variant">Grip or variant</Label>
					<Input id="{id}-variant" bind:value={form.variant} />
				</div>
			</div>
			<div class="flex flex-wrap items-end gap-4">
				<div class="flex items-center gap-2">
					<Checkbox id="{id}-unilateral" bind:checked={form.unilateral} />
					<Label for="{id}-unilateral" class="font-normal">One side at a time</Label>
				</div>
				<div class="grid gap-1.5">
					<Label for="{id}-basis">Weight is</Label>
					<Select.Root type="single" bind:value={form.load_basis} disabled={!form.unilateral}>
						<Select.Trigger id="{id}-basis" class="w-64 max-w-full"
							>{LOAD_BASES.find((b) => b.value === form.load_basis)?.label}</Select.Trigger
						>
						<Select.Content>
							{#each LOAD_BASES as b (b.value)}<Select.Item
									value={b.value}
									label={b.label}
								/>{/each}
						</Select.Content>
					</Select.Root>
				</div>
			</div>
			<div class="grid gap-1.5">
				<Label for="{id}-notes">Notes</Label>
				<Textarea id="{id}-notes" rows={2} bind:value={form.notes} />
			</div>
			<div>
				<Button variant="link" class="px-0" aria-expanded={more} onclick={() => (more = !more)}
					>{more ? 'Fewer fields' : 'More: cardio fields'}</Button
				>
			</div>
			{#if more}
				<div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
					{#each [['duration', 'Duration (min)', 'duration_min'], ['distance', 'Distance (km)', 'distance_km'], ['avg-hr', 'Average heart rate (bpm)', 'avg_hr'], ['max-hr', 'Max heart rate (bpm)', 'max_hr'], ['speed', 'Average speed (km/h)', 'avg_speed_kmh'], ['incline', 'Average incline (%)', 'avg_incline_pct'], ['calories', 'Calories (kcal)', 'calories']] as const as [key, label, field] (key)}
						{@const whole = field === 'avg_hr' || field === 'max_hr' || field === 'calories'}
						<div class="grid gap-1.5">
							<Label for="{id}-{key}">{label}</Label>
							<Input
								id="{id}-{key}"
								type="number"
								step={whole ? 1 : 'any'}
								class="tabular-nums"
								bind:value={form[field]}
							/>
						</div>
					{/each}
				</div>
			{/if}
			<p class="text-xs text-muted-foreground">Emptying a number keeps the old one.</p>
			<div class="flex gap-2">
				<Button type="submit" disabled={busy}>Save set</Button>
				<Button variant="ghost" onclick={() => (editing = false)}>Cancel</Button>
			</div>
		</form>
	{/if}
</li>
