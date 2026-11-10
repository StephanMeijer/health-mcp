<script lang="ts">
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { MUSCLE_GROUPS, muscleLabel } from '#lib/muscles.js';

	/**
	 * Pick muscle groups an exercise stores: the 18 fine names only, since coarse ones (back, legs) are filters.
	 * `value` is bindable; `disabled` greys out groups already in the other list (primary vs secondary can't overlap).
	 * `id` prefixes the checkbox ids so two inputs on one page stay distinct.
	 */
	let {
		label,
		id,
		value = $bindable([]),
		disabled = []
	}: { label: string; id: string; value?: string[]; disabled?: string[] } = $props();

	function toggle(m: string, on: boolean) {
		value = on ? [...value, m] : value.filter((v) => v !== m);
	}
</script>

<fieldset class="grid gap-2">
	<legend class="mb-2 text-sm font-medium">{label}</legend>
	<div class="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-3">
		{#each MUSCLE_GROUPS as m (m)}
			<div class="flex items-center gap-2">
				<Checkbox
					id="{id}-{m}"
					checked={value.includes(m)}
					disabled={disabled.includes(m) && !value.includes(m)}
					onCheckedChange={(on) => toggle(m, on)}
				/>
				<Label for="{id}-{m}" class="font-normal">{muscleLabel(m)}</Label>
			</div>
		{/each}
	</div>
</fieldset>
