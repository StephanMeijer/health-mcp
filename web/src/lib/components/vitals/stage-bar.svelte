<script lang="ts">
	import type { Stage } from '#lib/api/results.js';
	import { STAGES, num } from '#lib/vitals.js';

	/** How a night split into stages, as plain divs. `split` is seconds or percentages per stage. */
	let {
		split,
		legend = false
	}: { split: Partial<Record<Stage, number>> | null; legend?: boolean } = $props();

	let parts = $derived.by(() => {
		const total = STAGES.reduce((s, x) => s + (split?.[x.stage] ?? 0), 0);
		return total
			? STAGES.filter((x) => split?.[x.stage]).map((x) => ({
					...x,
					pct: (100 * split![x.stage]!) / total
				}))
			: [];
	});
	let summary = $derived(parts.map((p) => `${p.label} ${num(p.pct, 0)}%`).join(', '));
</script>

{#if parts.length}
	<div class="grid gap-1.5">
		<div
			class="flex h-2.5 w-full overflow-hidden rounded-full bg-muted"
			role="img"
			aria-label={summary}
		>
			{#each parts as p (p.stage)}
				<div class={p.color} style:width="{p.pct}%" title="{p.label} {num(p.pct, 0)}%"></div>
			{/each}
		</div>
		{#if legend}
			<ul class="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
				{#each parts as p (p.stage)}
					<li class="flex items-center gap-1.5">
						<span class="size-2 rounded-full {p.color}"></span>{p.label}
						<span class="tabular-nums">{num(p.pct, 0)}%</span>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
{:else}
	<span class="text-xs text-muted-foreground">No stages</span>
{/if}
