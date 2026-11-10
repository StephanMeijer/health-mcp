<script lang="ts">
	import { untrack } from 'svelte';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import type { SleepSession, Stage } from '#lib/api/results.js';
	import type { SleepAddArgs } from '#lib/api/tools.gen.js';
	import { STAGES, fromLocalInput, toLocalInput } from '#lib/vitals.js';

	/**
	 * Log a night, or edit `session`. Stages are optional; on an edit, rows given here replace the session's stages and
	 * leaving them empty keeps the ones it has.
	 */
	let {
		session = null,
		onsave,
		onclose
	}: {
		session?: SleepSession | null;
		onsave: (args: SleepAddArgs) => Promise<void>;
		onclose: () => void;
	} = $props();

	const initial = untrack(() => session);
	const today = new Date();
	const lastNight = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1, 23);
	let start = $state(toLocalInput(initial?.start_at ?? lastNight.toISOString()));
	let end = $state(
		toLocalInput(initial?.end_at ?? new Date(lastNight.getTime() + 8 * 3_600_000).toISOString())
	);
	let note = $state(initial?.note ?? '');
	let stages = $state<{ time: string; stage: Stage; minutes: number | null }[]>([]);
	let busy = $state(false);
	let invalid = $derived(start && end && new Date(end) <= new Date(start));

	function addStage() {
		const prev = stages.at(-1);
		const time = prev
			? toLocalInput(
					new Date(new Date(prev.time).getTime() + (prev.minutes ?? 0) * 60_000).toISOString()
				)
			: start;
		stages.push({ time, stage: 'light', minutes: null });
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (invalid) return;
		busy = true;
		try {
			await onsave({
				start_at: fromLocalInput(start),
				end_at: fromLocalInput(end),
				// On an edit '' clears the note: sleep_update keeps the old one for null.
				note: session ? note.trim() : note.trim() || null,
				stages: stages.map((s) => ({
					time: fromLocalInput(s.time),
					stage: s.stage,
					duration_s: Math.round((s.minutes ?? 0) * 60)
				}))
			});
		} finally {
			busy = false;
		}
	}
</script>

<Dialog.Root open onOpenChange={(o) => !o && onclose()}>
	<Dialog.Content class="sm:max-w-xl">
		<Dialog.Header>
			<Dialog.Title>{session ? 'Edit sleep' : 'Log sleep'}</Dialog.Title>
			<Dialog.Description
				>When you fell asleep and woke up, in your own timezone.</Dialog.Description
			>
		</Dialog.Header>
		<form class="grid gap-4" onsubmit={submit}>
			<div class="grid gap-3 sm:grid-cols-2">
				<div class="grid gap-2">
					<Label for="sleep-start">Fell asleep</Label>
					<Input id="sleep-start" type="datetime-local" bind:value={start} required />
				</div>
				<div class="grid gap-2">
					<Label for="sleep-end">Woke up</Label>
					<Input
						id="sleep-end"
						type="datetime-local"
						bind:value={end}
						required
						aria-invalid={invalid || undefined}
						aria-describedby={invalid ? 'sleep-end-error' : undefined}
					/>
				</div>
			</div>
			{#if invalid}
				<p id="sleep-end-error" class="text-sm text-destructive">
					Waking up has to come after falling asleep.
				</p>
			{/if}
			<div class="grid gap-2">
				<Label for="sleep-note">Note (optional)</Label>
				<Input id="sleep-note" bind:value={note} placeholder="Late dinner, woke up once…" />
			</div>
			<fieldset class="grid gap-2">
				<legend class="mb-2 text-sm font-medium">Stages (optional)</legend>
				{#if session && !stages.length}
					<p class="text-xs text-muted-foreground">
						Leave this empty to keep the stages it has; stages you add here replace them.
					</p>
				{/if}
				{#each stages as s, i (i)}
					<div class="grid grid-cols-[1fr_7rem_6rem_auto] items-end gap-2">
						<div class="grid gap-1">
							<Label for="stage-time-{i}" class="text-xs">Start</Label>
							<Input id="stage-time-{i}" type="datetime-local" bind:value={s.time} required />
						</div>
						<div class="grid gap-1">
							<Label for="stage-kind-{i}" class="text-xs">Stage</Label>
							<Select.Root type="single" bind:value={s.stage}>
								<Select.Trigger id="stage-kind-{i}" class="w-full">
									{STAGES.find((x) => x.stage === s.stage)?.label}
								</Select.Trigger>
								<Select.Content>
									{#each STAGES as x (x.stage)}
										<Select.Item value={x.stage} label={x.label}>{x.label}</Select.Item>
									{/each}
								</Select.Content>
							</Select.Root>
						</div>
						<div class="grid gap-1">
							<Label for="stage-min-{i}" class="text-xs">Minutes</Label>
							<Input
								id="stage-min-{i}"
								type="number"
								min="1"
								step="1"
								bind:value={s.minutes}
								required
							/>
						</div>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							aria-label="Remove stage {i + 1}"
							onclick={() => stages.splice(i, 1)}>Remove</Button
						>
					</div>
				{/each}
				<div>
					<Button type="button" variant="outline" size="sm" onclick={addStage}>Add stage</Button>
				</div>
			</fieldset>
			<Dialog.Footer>
				<Button type="button" variant="outline" onclick={onclose}>Cancel</Button>
				<Button type="submit" disabled={busy || !!invalid}
					>{session ? 'Save sleep' : 'Log sleep'}</Button
				>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
