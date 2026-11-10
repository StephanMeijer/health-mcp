<script lang="ts">
	import { toolMutation } from '#lib/api/query.svelte.js';
	import type { Settings } from '#lib/api/results.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import MuscleSelect from './muscle-select.svelte';
	import { changedSettings, GOALS, settingsForm, type SettingsForm } from './settings.js';

	/** Edits `settings`; the fields reset when a stored value changes (a save here or elsewhere). */
	let { settings, readonly = false }: { settings: Settings; readonly?: boolean } = $props();

	// Every refetch brings a fresh `now`: comparing the stored fields as a string keeps edits in progress.
	const stored = $derived(JSON.stringify(settingsForm(settings)));
	let form = $derived.by(() => {
		const fields = $state<SettingsForm>(JSON.parse(stored));
		return fields;
	});
	const changes = $derived(changedSettings(settings, form));
	const dirty = $derived(Object.keys(changes).length > 0);

	const zones = $derived.by(() => {
		const all = Intl.supportedValuesOf('timeZone');
		// Postgres may use a name the browser lists under another one ("UTC", "Etc/UTC").
		return all.includes(settings.timezone) ? all : [settings.timezone, ...all];
	});

	const save = toolMutation<Settings>('settings_update', {
		invalidate: ['settings_get', 'training_overview'],
		success: 'Settings saved'
	});

	const today = $derived(
		new Date(settings.now).toLocaleString(undefined, {
			timeZone: settings.timezone,
			dateStyle: 'full',
			timeStyle: 'short'
		})
	);
</script>

<form
	class="grid max-w-2xl gap-6"
	onsubmit={(e) => {
		e.preventDefault();
		if (dirty) save.mutate(changes);
	}}
>
	<fieldset class="grid gap-6" disabled={readonly}>
		<div class="grid gap-2">
			<Label for="timezone">Timezone</Label>
			<Select.Root type="single" bind:value={form.timezone} disabled={readonly}>
				<Select.Trigger id="timezone" class="w-full sm:w-80">{form.timezone}</Select.Trigger>
				<Select.Content class="max-h-80">
					{#each zones as z (z)}
						<Select.Item value={z}>{z}</Select.Item>
					{/each}
				</Select.Content>
			</Select.Root>
			<p class="text-sm text-muted-foreground">
				Now in {settings.timezone}: <span class="tabular-nums">{today}</span>. Days, weeks and local
				times you log follow this timezone.
			</p>
		</div>

		<div class="grid gap-2">
			<Label for="goal">Training goal</Label>
			<Select.Root type="single" bind:value={form.goal} disabled={readonly}>
				<Select.Trigger id="goal" class="w-full sm:w-80">
					{GOALS.find((g) => g.value === form.goal)?.label ?? 'Not set'}
				</Select.Trigger>
				<Select.Content>
					{#each GOALS as g (g.value)}
						<Select.Item value={g.value}>{g.label}</Select.Item>
					{/each}
				</Select.Content>
			</Select.Root>
			<p class="text-sm text-muted-foreground">Sets the rep ranges and progression suggested.</p>
		</div>

		<div class="grid gap-2">
			<Label for="goals_notes">Goals and plans</Label>
			<Textarea
				id="goals_notes"
				bind:value={form.goals_notes}
				placeholder="Hyrox on 14 March, bench 100 kg by summer"
			/>
		</div>

		<div class="grid gap-2">
			<span class="text-sm font-medium">Muscles to prioritise</span>
			<MuscleSelect bind:value={form.priority_muscle_groups} disabled={readonly} />
		</div>

		<div class="grid gap-2">
			<Label for="limitations">Injuries and limitations</Label>
			<Textarea
				id="limitations"
				bind:value={form.limitations}
				placeholder="Left shoulder: no overhead pressing"
			/>
			<p class="text-sm text-muted-foreground">
				Exercises that conflict with these aren't suggested.
			</p>
		</div>
	</fieldset>

	{#if !readonly}
		<div class="flex gap-2">
			<Button type="submit" disabled={!dirty || save.isPending}>Save settings</Button>
			<Button
				type="button"
				variant="ghost"
				disabled={!dirty}
				onclick={() => Object.assign(form, settingsForm(settings))}>Undo changes</Button
			>
		</div>
	{/if}
</form>
