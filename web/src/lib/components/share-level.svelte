<script lang="ts" module>
	import type { Access } from '#lib/logbook.svelte.js';

	export const LEVELS: Record<Access, string> = { none: 'None', ro: 'Read', rw: 'Read & write' };
</script>

<script lang="ts">
	import * as Select from '#lib/components/ui/select/index.js';

	/** How much of one domain (gym or vitals) someone may see: none, read, or read & write. */
	let {
		value = $bindable(),
		label,
		id,
		onchange
	}: { value: Access; label: string; id?: string; onchange?: (v: Access) => unknown } = $props();

	function set(v: Access) {
		const prev = value;
		value = v;
		// A rejected save shows the stored level again.
		Promise.resolve(onchange?.(v)).catch(() => (value = prev));
	}
</script>

<Select.Root type="single" bind:value={() => value, (v) => set(v as Access)}>
	<Select.Trigger {id} aria-label={label} class="w-36">{LEVELS[value]}</Select.Trigger>
	<Select.Content>
		{#each Object.entries(LEVELS) as [v, text] (v)}
			<Select.Item value={v}>{text}</Select.Item>
		{/each}
	</Select.Content>
</Select.Root>
