<script lang="ts">
	/** Stands in for EChart in jsdom (no canvas): keeps its accessible label to assert on. */
	let { label }: { label: string } = $props();
</script>

<div role="img" aria-label={label}></div>
