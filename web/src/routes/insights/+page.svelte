<script lang="ts">
	import { page } from '$app/state';
	import PageHeader from '#lib/components/page-header.svelte';
	import * as Tabs from '#lib/components/ui/tabs/index.js';
	import HeartRate from './heart-rate.svelte';
	import Strength from './strength.svelte';
	import Volume from './volume.svelte';
</script>

<PageHeader title="Insights" description="How your training and body change over time." />

<!-- ?tab=volume opens a tab directly. -->
<Tabs.Root value={page.url.searchParams.get('tab') ?? 'strength'}>
	<Tabs.List class="mb-4">
		<Tabs.Trigger value="strength">Strength</Tabs.Trigger>
		<Tabs.Trigger value="volume">Volume</Tabs.Trigger>
		<Tabs.Trigger value="heart-rate">Heart rate</Tabs.Trigger>
	</Tabs.List>

	<Tabs.Content value="strength"><Strength /></Tabs.Content>
	<Tabs.Content value="volume"><Volume /></Tabs.Content>
	<Tabs.Content value="heart-rate"><HeartRate /></Tabs.Content>
</Tabs.Root>
