<script lang="ts">
	import { page } from '$app/state';
	import { toolQuery } from '#lib/api/query.svelte.js';
	import type { Settings } from '#lib/api/results.js';
	import PageHeader from '#lib/components/page-header.svelte';
	import ReadOnlyNote from '#lib/components/read-only-note.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Tabs from '#lib/components/ui/tabs/index.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import { logout } from '#lib/server-routes.js';
	import BodyNotes from './body-notes.svelte';
	import SettingsForm from './settings-form.svelte';

	const settings = toolQuery<Settings>('settings_get', () => ({}));
	const readonly = $derived(!logbook.canWrite('gym'));
</script>

<PageHeader
	title="Settings"
	description={logbook.owner
		? `Training settings and body notes of ${logbook.owner}.`
		: 'Your training settings, body notes and account.'}
/>
<ReadOnlyNote domain="gym" />

<!-- ?tab=body-notes opens a tab directly. -->
<Tabs.Root value={page.url.searchParams.get('tab') ?? 'settings'}>
	<Tabs.List class="mb-4">
		<Tabs.Trigger value="settings">Training</Tabs.Trigger>
		<Tabs.Trigger value="body-notes">Body notes</Tabs.Trigger>
		<Tabs.Trigger value="account">Account</Tabs.Trigger>
	</Tabs.List>

	<Tabs.Content value="settings">
		{#if settings.isPending}
			<div class="grid max-w-2xl gap-4">
				{#each [1, 2, 3, 4] as i (i)}<Skeleton class="h-16 w-full" />{/each}
			</div>
		{:else if settings.isError}
			<p class="text-sm text-destructive">
				Settings could not be loaded: {settings.error.message}
			</p>
		{:else}
			<SettingsForm settings={settings.data} {readonly} />
		{/if}
	</Tabs.Content>

	<Tabs.Content value="body-notes">
		<BodyNotes {readonly} />
	</Tabs.Content>

	<Tabs.Content value="account">
		<Card.Root class="max-w-md">
			<Card.Header>
				<Card.Title>Account</Card.Title>
				<Card.Description>
					Signed in as <b class="text-foreground"
						>{logbook.me?.email ?? 'an account without email'}</b
					>.
				</Card.Description>
			</Card.Header>
			<Card.Footer>
				<Button variant="outline" onclick={logout}>Log out</Button>
			</Card.Footer>
		</Card.Root>
	</Tabs.Content>
</Tabs.Root>
