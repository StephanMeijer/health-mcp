<script lang="ts">
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { COARSE_GROUPS, MUSCLE_GROUPS, muscleLabel } from '#lib/muscles.js';

	/**
	 * Priority muscle groups: the fine groups plus the coarse names, which settings_update expands.
	 * shortcut: local to settings; switch to the shared muscle-groups-input from #103 once it lands.
	 */
	let { value = $bindable([]), disabled = false }: { value?: string[]; disabled?: boolean } =
		$props();

	const groups = [
		{ heading: 'Whole areas', names: Object.keys(COARSE_GROUPS) },
		{ heading: 'Single muscles', names: [...MUSCLE_GROUPS] }
	];

	function toggle(name: string, on: boolean) {
		value = on ? [...value, name] : value.filter((v) => v !== name);
	}
</script>

{#each groups as g (g.heading)}
	<fieldset class="grid gap-2" {disabled}>
		<legend class="mb-2 text-sm text-muted-foreground">{g.heading}</legend>
		<div class="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-3 lg:grid-cols-4">
			{#each g.names as name (name)}
				<div class="flex items-center gap-2">
					<Checkbox
						id="muscle-{name}"
						checked={value.includes(name)}
						onCheckedChange={(on) => toggle(name, on)}
						{disabled}
					/>
					<Label for="muscle-{name}" class="font-normal">{muscleLabel(name)}</Label>
				</div>
			{/each}
		</div>
	</fieldset>
{/each}
