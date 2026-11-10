<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import * as Sidebar from '#lib/components/ui/sidebar/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import { logout } from '#lib/server-routes.js';
	import { toggleMode } from 'mode-watcher';
	import {
		Dumbbell,
		ListChecks,
		MapPin,
		HeartPulse,
		ChartLine,
		Users,
		Settings,
		ChevronsUpDown,
		LogOut,
		SunMoon
	} from '@lucide/svelte';

	const nav = [
		{ href: '/workouts', label: 'Workouts', icon: Dumbbell },
		{ href: '/exercises', label: 'Exercises', icon: ListChecks },
		{ href: '/gyms', label: 'Gyms', icon: MapPin },
		{ href: '/vitals', label: 'Vitals', icon: HeartPulse },
		{ href: '/insights', label: 'Insights', icon: ChartLine },
		{ href: '/sharing', label: 'Sharing', icon: Users },
		{ href: '/settings', label: 'Settings', icon: Settings }
	] as const;

	const MINE = 'mine';
	let selected = $derived(logbook.owner ?? MINE);
	const label = (v: string) => (v === MINE ? 'Your logbook' : v);
</script>

<Sidebar.Root>
	<Sidebar.Header>
		{#if logbook.me?.shared_with_you.length}
			<Select.Root
				type="single"
				value={selected}
				onValueChange={(v) => (logbook.owner = v === MINE ? null : v)}
			>
				<Select.Trigger class="w-full" aria-label="Logbook">{label(selected)}</Select.Trigger>
				<Select.Content>
					<Select.Item value={MINE}>Your logbook</Select.Item>
					{#each logbook.me.shared_with_you as s (s.owner)}
						<Select.Item value={s.owner}>{s.owner}</Select.Item>
					{/each}
				</Select.Content>
			</Select.Root>
		{:else}
			<span class="px-2 py-1.5 text-sm font-semibold">Health</span>
		{/if}
	</Sidebar.Header>
	<Sidebar.Content>
		<Sidebar.Group>
			<Sidebar.GroupContent>
				<Sidebar.Menu>
					{#each nav as item (item.href)}
						<Sidebar.MenuItem>
							<Sidebar.MenuButton isActive={page.url.pathname.startsWith(resolve(item.href))}>
								{#snippet child({ props })}
									<a href={resolve(item.href)} {...props}>
										<item.icon />
										<span>{item.label}</span>
									</a>
								{/snippet}
							</Sidebar.MenuButton>
						</Sidebar.MenuItem>
					{/each}
				</Sidebar.Menu>
			</Sidebar.GroupContent>
		</Sidebar.Group>
	</Sidebar.Content>
	<Sidebar.Footer>
		<Sidebar.Menu>
			<Sidebar.MenuItem>
				<DropdownMenu.Root>
					<DropdownMenu.Trigger>
						{#snippet child({ props })}
							<Sidebar.MenuButton {...props} size="lg">
								<span class="truncate text-sm">{logbook.me?.email ?? 'Signed in'}</span>
								<ChevronsUpDown class="ml-auto" />
							</Sidebar.MenuButton>
						{/snippet}
					</DropdownMenu.Trigger>
					<DropdownMenu.Content side="top" align="start" class="w-56">
						<DropdownMenu.Item onclick={toggleMode}><SunMoon /> Light or dark</DropdownMenu.Item>
						<DropdownMenu.Separator />
						<DropdownMenu.Item onclick={logout}><LogOut /> Log out</DropdownMenu.Item>
					</DropdownMenu.Content>
				</DropdownMenu.Root>
			</Sidebar.MenuItem>
		</Sidebar.Menu>
	</Sidebar.Footer>
	<Sidebar.Rail />
</Sidebar.Root>
