<script lang="ts">
	import { page } from '$app/state';
	import PageHeader from '#lib/components/page-header.svelte';
	import * as Tabs from '#lib/components/ui/tabs/index.js';
	import Strength from './strength.svelte';
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
	<Tabs.Content value="volume">
		<p class="text-sm text-muted-foreground">
			Weekly sets per muscle, volume against HRV and a training calendar are coming in the next
			update.
		</p>
	</Tabs.Content>
	<Tabs.Content value="heart-rate">
		<p class="text-sm text-muted-foreground">
			A zoomable day of heart rate, ECG and movement is coming in the next update.
		</p>
	</Tabs.Content>
</Tabs.Root>
