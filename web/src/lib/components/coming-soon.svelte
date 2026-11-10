<script lang="ts">
	let { title, issue }: { title: string; issue: number } = $props();
</script>

<h1 class="text-2xl font-semibold tracking-tight">{title}</h1>
<p class="mt-2 text-muted-foreground">This screen arrives with issue #{issue}.</p>
