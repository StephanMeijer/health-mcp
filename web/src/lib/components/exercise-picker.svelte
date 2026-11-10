<script lang="ts">
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { toolQuery } from '#lib/api/query.svelte.js';
	import type { Exercise } from '#lib/api/results.js';

	/**
	 * Find an exercise by typing (the server tolerates typos) and pick it with the mouse or arrow keys + Enter.
	 * `onpick` gets the whole exercise; the box then clears. Needs a QueryClientProvider above it.
	 */
	let {
		onpick,
		label = 'Exercise',
		id = 'exercise-picker',
		placeholder = 'Type an exercise name'
	}: {
		onpick: (e: Exercise) => void;
		label?: string;
		id?: string;
		placeholder?: string;
	} = $props();

	let text = $state('');
	let query = $state('');
	let active = $state(0);
	let open = $state(false);

	$effect(() => {
		const q = text.trim();
		const t = setTimeout(() => (query = q), 250);
		return () => clearTimeout(t);
	});

	const results = toolQuery<Exercise[], 'exercise_search'>('exercise_search', () => ({ query }), {
		enabled: () => query !== ''
	});
	let options = $derived(query && results.data ? results.data.slice(0, 8) : []);

	function pick(e: Exercise) {
		onpick(e);
		text = query = '';
		open = false;
	}

	function onkeydown(ev: KeyboardEvent) {
		if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
			ev.preventDefault();
			open = true;
			const step = ev.key === 'ArrowDown' ? 1 : -1;
			active = (active + step + options.length) % Math.max(options.length, 1);
		} else if (ev.key === 'Enter' && open && options[active]) {
			ev.preventDefault();
			pick(options[active]);
		} else if (ev.key === 'Escape') {
			open = false;
		}
	}
</script>

<div class="relative grid gap-2">
	<Label for={id}>{label}</Label>
	<Input
		{id}
		role="combobox"
		autocomplete="off"
		aria-expanded={open && options.length > 0}
		aria-controls="{id}-list"
		aria-activedescendant={open && options[active] ? `${id}-${options[active].id}` : undefined}
		{placeholder}
		bind:value={text}
		oninput={() => {
			open = true;
			active = 0;
		}}
		onblur={() => (open = false)}
		{onkeydown}
	/>
	{#if open && options.length}
		<ul
			id="{id}-list"
			role="listbox"
			aria-label={label}
			class="absolute top-full z-50 mt-1 w-full overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
		>
			{#each options as e, i (e.id)}
				<li
					id="{id}-{e.id}"
					role="option"
					aria-selected={i === active}
					class="cursor-pointer rounded-sm px-2 py-1.5 text-sm aria-selected:bg-accent aria-selected:text-accent-foreground"
					onmousedown={(ev) => {
						ev.preventDefault(); // keep focus in the input so blur doesn't close the list first
						pick(e);
					}}
					onmousemove={() => (active = i)}
				>
					{e.name}
					{#if e.equipment}<span class="text-muted-foreground"> · {e.equipment}</span>{/if}
				</li>
			{/each}
		</ul>
	{:else if open && query && results.isSuccess && query === text.trim()}
		<p class="text-sm text-muted-foreground">No exercise matches “{query}”.</p>
	{/if}
</div>
