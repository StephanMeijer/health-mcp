<script lang="ts">
	import { logbook, type Domain } from '#lib/logbook.svelte.js';

	/** Shown above a screen whose data the current logbook shares read-only. */
	let { domain }: { domain: Domain } = $props();
</script>

{#if logbook.owner && !logbook.canWrite(domain)}
	<p class="mb-4 rounded-md border bg-muted px-3 py-2 text-sm text-muted-foreground">
		{logbook.owner} shares this {domain === 'gym' ? 'logbook' : 'vitals data'} with you read-only.
	</p>
{/if}
