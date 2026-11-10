<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import MuscleGroupsInput from '#lib/components/muscle-groups-input.svelte';
	import type { Exercise } from '#lib/api/results.js';
	import { splitList } from '#lib/utils.js';

	type Fields = Omit<Exercise, 'id'>;

	/**
	 * The fields of an exercise, for creating (empty) or editing (`exercise`). `onsubmit` gets every field;
	 * empty texts are '' so an edit can clear them. `readonly` shows the values without a save button.
	 */
	let {
		exercise,
		submitLabel,
		pending = false,
		readonly = false,
		onsubmit
	}: {
		exercise?: Exercise;
		submitLabel: string;
		pending?: boolean;
		readonly?: boolean;
		onsubmit: (fields: Fields) => void;
	} = $props();

	// The form edits a copy; the parent remounts it (a {#key}) to start over from new data.
	// svelte-ignore state_referenced_locally
	const e = exercise;
	let name = $state(e?.name ?? '');
	let aliases = $state(e?.aliases.join(', ') ?? '');
	let kind = $state<Exercise['kind']>(e?.kind ?? 'strength');
	let primary = $state(e?.muscle_groups ?? []);
	let secondary = $state(e?.secondary_muscle_groups ?? []);
	let equipment = $state(e?.equipment ?? '');
	let description = $state(e?.description ?? '');

	const kinds = { strength: 'Strength (reps and weight)', cardio: 'Cardio (time and distance)' };
</script>

<form
	class="grid gap-5"
	onsubmit={(ev) => {
		ev.preventDefault();
		onsubmit({
			name: name.trim(),
			aliases: splitList(aliases),
			kind,
			muscle_groups: primary,
			secondary_muscle_groups: secondary,
			equipment: equipment.trim(),
			description: description.trim()
		});
	}}
>
	<fieldset disabled={readonly} class="grid gap-5">
		<div class="grid gap-2">
			<Label for="ex-name">Name</Label>
			<Input id="ex-name" required bind:value={name} placeholder="Dumbbell bench press" />
		</div>
		<div class="grid gap-2">
			<Label for="ex-aliases">Other names</Label>
			<Input id="ex-aliases" bind:value={aliases} placeholder="db bench, flat db press" />
			<p class="text-xs text-muted-foreground">Separate with commas.</p>
		</div>
		<div class="grid gap-2">
			<Label for="ex-kind">Kind</Label>
			<Select.Root type="single" bind:value={kind}>
				<Select.Trigger id="ex-kind" class="w-full sm:w-72">{kinds[kind]}</Select.Trigger>
				<Select.Content>
					<Select.Item value="strength">{kinds.strength}</Select.Item>
					<Select.Item value="cardio">{kinds.cardio}</Select.Item>
				</Select.Content>
			</Select.Root>
		</div>
		<MuscleGroupsInput
			id="ex-primary"
			label="Target muscles"
			bind:value={primary}
			disabled={secondary}
		/>
		<MuscleGroupsInput
			id="ex-secondary"
			label="Muscles that assist"
			bind:value={secondary}
			disabled={primary}
		/>
		<div class="grid gap-2">
			<Label for="ex-equipment">Equipment it needs</Label>
			<Input id="ex-equipment" bind:value={equipment} placeholder="Dumbbells + flat bench" />
			<p class="text-xs text-muted-foreground">Leave empty for bodyweight.</p>
		</div>
		<div class="grid gap-2">
			<Label for="ex-description">Description</Label>
			<Textarea
				id="ex-description"
				bind:value={description}
				placeholder="What it looks like and how it's set up, so it can be recognised from a photo."
			/>
		</div>
	</fieldset>
	{#if !readonly}
		<div><Button type="submit" disabled={pending}>{submitLabel}</Button></div>
	{/if}
</form>
