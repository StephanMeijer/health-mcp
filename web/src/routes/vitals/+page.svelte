<script lang="ts">
	import { resolve } from '$app/paths';
	import * as Card from '#lib/components/ui/card/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { buttonVariants } from '#lib/components/ui/button/index.js';
	import PageHeader from '#lib/components/page-header.svelte';
	import EmptyState from '#lib/components/empty-state.svelte';
	import ReadOnlyNote from '#lib/components/read-only-note.svelte';
	import StageBar from '#lib/components/vitals/stage-bar.svelte';
	import { toolQuery } from '#lib/api/query.svelte.js';
	import type { ReadingRow, Summary } from '#lib/api/results.js';
	import { KINDS, type KindInfo } from '#lib/kinds.js';
	import { dateTime, num, readingValue } from '#lib/vitals.js';

	type Stats = { avg_7d: number | null; avg_30d: number | null; n_30d: number };

	const summary = toolQuery<Summary, 'summary'>('summary', () => ({}));

	let cards = $derived(
		KINDS.flatMap((info) => {
			const entry = summary.data?.[info.kind];
			const latest = entry?.latest as ReadingRow | null | undefined;
			return latest ? [{ info, latest, stats: entry![info.field] as Stats | null }] : [];
		})
	);
	let others = $derived(KINDS.filter((k) => !cards.some((c) => c.info === k)));
	let sleep = $derived(summary.data?.sleep);
</script>

<PageHeader
	title="Vitals"
	description="Your latest readings with 7- and 30-day averages, and your sleep."
/>
<ReadOnlyNote domain="vitals" />

{#snippet kindLink(info: KindInfo)}
	<a
		href={resolve('/vitals/[kind]', { kind: info.kind })}
		class={buttonVariants({ variant: 'outline', size: 'sm' })}>{info.label}</a
	>
{/snippet}

{#if summary.isPending}
	<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
		{#each { length: 6 }, i (i)}<Skeleton class="h-40" />{/each}
	</div>
{:else if summary.isError}
	<p class="text-sm text-destructive">Your vitals could not be loaded: {summary.error.message}</p>
{:else}
	<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
		<a
			href={resolve('/vitals/sleep')}
			class="rounded-xl focus-visible:ring-2 focus-visible:ring-ring"
		>
			<Card.Root class="h-full transition-colors hover:bg-muted/40">
				<Card.Header>
					<Card.Description>Sleep</Card.Description>
					<Card.Title class="font-numerals text-3xl tabular-nums">
						{#if sleep?.nights_30d}
							{num(sleep.avg_hours_7d ?? sleep.avg_hours_30d)}
							<span class="text-base font-normal text-muted-foreground">h a night</span>
						{:else}
							<span class="text-base font-normal text-muted-foreground">No nights yet</span>
						{/if}
					</Card.Title>
				</Card.Header>
				<Card.Content class="grid gap-3 text-sm">
					{#if sleep?.nights_30d}
						<dl class="grid grid-cols-3 gap-2">
							<div>
								<dt class="text-muted-foreground">7 days</dt>
								<dd class="tabular-nums">{num(sleep.avg_hours_7d)} h</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">30 days</dt>
								<dd class="tabular-nums">{num(sleep.avg_hours_30d)} h</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">Nights</dt>
								<dd class="tabular-nums">{sleep.nights_30d}</dd>
							</div>
						</dl>
						<StageBar split={sleep.stage_pct_30d} legend />
					{:else}
						<p class="text-muted-foreground">
							Nights you log or sync in the last 30 days show up here.
						</p>
					{/if}
				</Card.Content>
			</Card.Root>
		</a>
		{#each cards as { info, latest, stats } (info.kind)}
			<a
				href={resolve('/vitals/[kind]', { kind: info.kind })}
				class="rounded-xl focus-visible:ring-2 focus-visible:ring-ring"
			>
				<Card.Root class="h-full transition-colors hover:bg-muted/40">
					<Card.Header>
						<Card.Description>{info.label}</Card.Description>
						<Card.Title class="font-numerals text-3xl tabular-nums">
							{num(readingValue(latest, info), 2)}
							<span class="text-base font-normal text-muted-foreground">{info.unit}</span>
						</Card.Title>
						<p class="text-xs text-muted-foreground">{dateTime(latest.time)}</p>
					</Card.Header>
					<Card.Content class="text-sm">
						<dl class="grid grid-cols-3 gap-2">
							<div>
								<dt class="text-muted-foreground">7-day avg</dt>
								<dd class="tabular-nums">{num(stats?.avg_7d)}</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">30-day avg</dt>
								<dd class="tabular-nums">{num(stats?.avg_30d)}</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">Readings</dt>
								<dd class="tabular-nums">{stats?.n_30d ?? 0}</dd>
							</div>
						</dl>
					</Card.Content>
				</Card.Root>
			</a>
		{/each}
	</div>

	{#if !cards.length}
		<div class="mt-6">
			<EmptyState
				title="No readings yet"
				hint="Heart rate, weight, blood pressure and the rest show up here once they are logged. Pick a vital below to add the first reading."
			/>
		</div>
	{/if}

	<section class="mt-8 grid gap-3">
		<h2 class="text-sm font-medium text-muted-foreground">
			{cards.length ? 'Other vitals' : 'Vitals you can log'}
		</h2>
		<div class="flex flex-wrap gap-2">
			{#each others as info (info.kind)}{@render kindLink(info)}{/each}
		</div>
	</section>
{/if}
