<script lang="ts">
	import './layout.css';
	import favicon from '#lib/assets/favicon.svg';
	import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
	import { ModeWatcher } from 'mode-watcher';
	import * as Sidebar from '#lib/components/ui/sidebar/index.js';
	import { Toaster } from '#lib/components/ui/sonner/index.js';
	import AppSidebar from '#lib/components/app-sidebar.svelte';
	import { retryable } from '#lib/api/client.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import type { LayoutProps } from './$types';

	let { children, data }: LayoutProps = $props();

	$effect.pre(() => {
		logbook.me = data.me;
	});

	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: retryable, staleTime: 30_000 } }
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<ModeWatcher />
<QueryClientProvider client={queryClient}>
	<Sidebar.Provider>
		<AppSidebar />
		<Sidebar.Inset>
			<header class="flex h-12 items-center gap-2 border-b px-3">
				<Sidebar.Trigger />
				{#if logbook.owner}
					<span class="text-sm text-muted-foreground">
						Viewing the logbook of <b class="text-foreground">{logbook.owner}</b>
					</span>
				{/if}
			</header>
			<main class="p-4 md:p-6">{@render children()}</main>
		</Sidebar.Inset>
	</Sidebar.Provider>
	<Toaster richColors />
</QueryClientProvider>
