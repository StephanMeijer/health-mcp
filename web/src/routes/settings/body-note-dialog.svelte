<script lang="ts">
	import { toolMutation } from '#lib/api/query.svelte.js';
	import type { BodyNote } from '#lib/api/results.js';
	import { Button, buttonVariants } from '#lib/components/ui/button/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import { MUSCLE_GROUPS } from '#lib/muscles.js';
	import { addNoteArgs, noteForm, updateNoteArgs } from './settings.js';

	/** Adds a body note, or edits `note` when given. */
	let { note }: { note?: BodyNote } = $props();

	let open = $state(false);
	let form = $state(noteForm());
	const id = $derived(note ? `note-${note.id}` : 'note-new');

	const opts = { invalidate: ['body_note_list' as const, 'training_overview' as const] };
	const add = toolMutation<BodyNote>('body_note_add', { ...opts, success: 'Body note added' });
	const update = toolMutation<BodyNote>('body_note_update', {
		...opts,
		success: 'Body note saved'
	});

	function submit(e: SubmitEvent) {
		e.preventDefault();
		// On an error the toast says why and the dialog stays open with the input.
		const close = { onSuccess: () => (open = false) };
		if (note) update.mutate(updateNoteArgs(note, form), close);
		else add.mutate(addNoteArgs(form), close);
	}
</script>

<Dialog.Root bind:open onOpenChange={(o) => o && (form = noteForm(note))}>
	<Dialog.Trigger class={buttonVariants({ variant: note ? 'ghost' : 'default', size: 'sm' })}>
		{note ? 'Edit' : 'Add body note'}
	</Dialog.Trigger>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>{note ? 'Edit body note' : 'Add body note'}</Dialog.Title>
			<Dialog.Description>An ache, pain or fatigue: where, how bad and when.</Dialog.Description>
		</Dialog.Header>
		<form class="grid gap-4" onsubmit={submit}>
			<div class="grid gap-2">
				<Label for="{id}-location">Where</Label>
				<Input
					id="{id}-location"
					list="{id}-muscles"
					bind:value={form.location}
					placeholder="left knee, lower_back"
					required
				/>
				<datalist id="{id}-muscles">
					{#each MUSCLE_GROUPS as m (m)}<option value={m}></option>{/each}
				</datalist>
			</div>
			<div class="grid gap-2">
				<Label for="{id}-severity">How bad (1–10, optional)</Label>
				<Input
					id="{id}-severity"
					type="number"
					min="1"
					max="10"
					step="1"
					bind:value={form.severity}
					class="w-24 tabular-nums"
				/>
				<p class="text-sm text-muted-foreground">
					1 is barely noticeable, 10 is can’t train it. Leave empty for none.
				</p>
			</div>
			<div class="grid gap-2">
				<Label for="{id}-at">When</Label>
				<Input id="{id}-at" type="datetime-local" bind:value={form.noted_at} class="w-fit" />
				{#if !note}<p class="text-sm text-muted-foreground">Leave empty for now.</p>{/if}
			</div>
			<div class="grid gap-2">
				<Label for="{id}-notes">Notes</Label>
				<Textarea id="{id}-notes" bind:value={form.notes} />
			</div>
			<Dialog.Footer>
				<Button type="submit" disabled={add.isPending || update.isPending}>
					{note ? 'Save body note' : 'Add body note'}
				</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
