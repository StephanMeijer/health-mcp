<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import { toolMutation } from '#lib/api/query.svelte.js';
	import type { Gym } from '#lib/api/results.js';

	/**
	 * A gym's equipment text, with the server's two ways to change it: "Add a line" (equipment_append) and
	 * "Edit all" (equipment, replaces the text). `editable` false shows the text only.
	 */
	let { gym, editable }: { gym: Gym; editable: boolean } = $props();

	let line = $state('');
	let editing = $state(false);
	let all = $state('');

	const update = toolMutation<Gym, 'gym_update'>('gym_update', {
		invalidate: ['gym_get', 'gym_list'],
		success: 'Equipment saved'
	});

	function append(ev: SubmitEvent) {
		ev.preventDefault();
		update.mutate(
			{ gym: String(gym.id), equipment_append: line.trim() },
			{ onSuccess: () => (line = '') }
		);
	}

	function replace(ev: SubmitEvent) {
		ev.preventDefault();
		update.mutate(
			{ gym: String(gym.id), equipment: all.trim() },
			{ onSuccess: () => (editing = false) }
		);
	}
</script>

<section class="grid gap-3">
	<div class="flex items-center justify-between gap-2">
		<h2 class="text-lg font-semibold">Equipment</h2>
		{#if editable && !editing}
			<Button
				variant="outline"
				size="sm"
				onclick={() => {
					all = gym.equipment ?? '';
					editing = true;
				}}>Edit all</Button
			>
		{/if}
	</div>
	{#if editing}
		<form class="grid gap-2" onsubmit={replace}>
			<Label for="equipment-all">All equipment, one thing per line</Label>
			<Textarea id="equipment-all" rows={8} bind:value={all} />
			<div class="flex gap-2">
				<Button type="submit" disabled={update.isPending}>Save equipment</Button>
				<Button variant="ghost" onclick={() => (editing = false)}>Cancel</Button>
			</div>
		</form>
	{:else}
		{#if gym.equipment}
			<p class="rounded-md border bg-card p-3 text-sm whitespace-pre-line">{gym.equipment}</p>
		{:else}
			<p class="text-sm text-muted-foreground">
				No equipment written down yet. What the gym has (and lacks) helps Claude suggest exercises
				you can do there.
			</p>
		{/if}
		{#if editable}
			<form class="flex flex-wrap items-end gap-2" onsubmit={append}>
				<div class="grid min-w-64 flex-1 gap-2">
					<Label for="equipment-line">Add a line</Label>
					<Input
						id="equipment-line"
						bind:value={line}
						placeholder="Cable tower, 2 stacks up to 45 kg, near the mirrors"
					/>
				</div>
				<Button type="submit" disabled={!line.trim() || update.isPending}>Add line</Button>
			</form>
		{/if}
	{/if}
</section>
