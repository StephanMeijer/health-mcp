<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import type uPlot from 'uplot';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import EmptyState from '#lib/components/empty-state.svelte';
	import UPlot from '#lib/charts/uplot.svelte';
	import type { UplotTheme } from '#lib/charts/theme.js';
	import { isoDay } from '#lib/charts/volume.js';
	import {
		bucketFor,
		dayBounds,
		DETAIL_UP_TO,
		gapFor,
		keysFor,
		SIGNALS,
		stepText,
		toColumns,
		windowSql,
		type Row,
		type Signal
	} from '#lib/charts/heart-rate.js';
	import { tool } from '#lib/api/client.js';
	import { toolKey } from '#lib/api/query.svelte.js';
	import type { SqlRows } from '#lib/api/results.js';
	import { logbook } from '#lib/logbook.svelte.js';

	const MIN_SPAN = 2;
	// sql_query stops at 1000 rows: only when devices overlap, since windows are sized below that.
	const TRUNCATED = 'Too many readings in this window, so the chart stops early. Zoom in further.';
	const today = isoDay(new Date());
	let day = $state(today);
	let bounds = $derived(dayBounds(day));
	let view = $state<[number, number]>(dayBounds(today));
	let span = $derived(view[1] - view[0]);

	/** Moves the window inside the day; ignores a window the charts already show (their own setScale echoes). */
	function show(from: number, to: number) {
		const [lo, hi] = bounds;
		const s = Math.min(Math.max(to - from, MIN_SPAN), hi - lo);
		const f = Math.min(Math.max(from, lo), hi - s);
		if (Math.abs(f - view[0]) + Math.abs(f + s - view[1]) > span * 0.005) view = [f, f + s];
	}
	const zoom = (factor: number) => {
		const mid = (view[0] + view[1]) / 2;
		show(mid - (span * factor) / 2, mid + (span * factor) / 2);
	};
	const pan = (by: number) => show(view[0] + span * by, view[1] + span * by);

	// Dragging fires setScale on every redraw: refetch once the person stops.
	let timer: ReturnType<typeof setTimeout> | undefined;
	const zoomed = (u: uPlot, key: string) => {
		const { min, max } = u.scales.x;
		if (key !== 'x' || min == null || max == null) return;
		clearTimeout(timer);
		timer = setTimeout(() => show(min, max), 300);
	};
	$effect(() => () => clearTimeout(timer));

	/**
	 * One signal for the current window. The key holds the window, so an answer for an old window lands in its own
	 * cache entry and never on screen; the last window's chart stays up (same logbook only) while the new one loads.
	 */
	function signal(name: Signal, enabled: () => boolean = () => true) {
		return createQuery(() => {
			const [from, to] = view;
			const step = bucketFor(name, to - from);
			const sql = windowSql(name, from, to);
			return {
				queryKey: toolKey('sql_query', { sql }),
				queryFn: async () => ({
					step,
					...((await tool('sql_query', { sql }, { owner: logbook.owner })) as SqlRows<Row>)
				}),
				enabled: logbook.canRead('vitals') && enabled(),
				placeholderData: (prev, q) => (q?.queryKey[0] === logbook.owner ? prev : undefined)
			};
		});
	}
	const hr = signal('heart_rate');
	const ecg = signal('ecg', () => span <= DETAIL_UP_TO);
	const acc = signal('acc', () => span <= DETAIL_UP_TO);

	// shortcut: assumes the hsl() theme tokens layout.css uses; a hex token would need a real conversion.
	const faint = (c: string) => c.replace('hsl(', 'hsla(').replace(')', ', 0.2)');

	/** uPlot options for one signal at one bucket width: a line per column, a min–max band when bucketed. */
	function options(name: Signal, step: number, labels: string[], unit: string, per: number) {
		const fmt = (v: number) => `${Math.round((v / per) * 100) / 100} ${unit}`;
		const keys = keysFor(name, step);
		return (t: UplotTheme): Omit<uPlot.Options, 'width' | 'height'> => ({
			scales: { x: { time: true } },
			cursor: { sync: { key: 'heart-rate' }, drag: { x: true, y: false } },
			axes: [t.axis, { ...t.axis, size: 72, values: (_u, vs) => vs.map(fmt) }],
			series: [
				{},
				...keys.map((_, i) => ({
					label: keys.length > labels.length ? ['Average', 'Lowest', 'Highest'][i] : labels[i],
					stroke: t.colors[keys.length > labels.length ? 0 : i],
					width: i && keys.length > labels.length ? 0 : 1.5,
					value: (_u: uPlot, v: number | null) => (v == null ? '–' : fmt(v))
				}))
			],
			bands: keys.length > labels.length ? [{ series: [3, 2], fill: faint(t.colors[0]) }] : [],
			hooks: { setScale: [zoomed] }
		});
	}

	const time = (s: number) =>
		new Date(s * 1000).toLocaleTimeString(undefined, {
			hour: '2-digit',
			minute: '2-digit',
			...(span < 3600 && { second: '2-digit' })
		});
	// Series names, unit, and raw units per displayed unit (acc is stored in milli-g).
	const LABELS: Record<Signal, [string[], string, number]> = {
		heart_rate: [['Heart rate'], 'bpm', 1],
		ecg: [['ECG'], 'µV', 1],
		acc: [['X', 'Y', 'Z'], 'g', 1000]
	};
	const chart = (name: Signal, res: { step: number; rows: Row[] }) => ({
		options: options(name, res.step, ...LABELS[name]),
		data: toColumns(res.rows, keysFor(name, res.step), gapFor(name, res.step)) as uPlot.AlignedData,
		window: view
	});
	const DETAILS = [
		{
			name: 'ecg',
			q: ecg,
			title: 'ECG',
			what: 'the ECG',
			hint: 'A chest strap such as the Polar H10 records ECG while the app streams it.'
		},
		{
			name: 'acc',
			q: acc,
			title: 'Movement',
			what: 'movement',
			hint: 'A chest strap records acceleration while the app streams it; 1 g is gravity.'
		}
	] as const;
	const detail = (name: Signal, step = bucketFor(name, span)) =>
		!step
			? 'every sample'
			: `${stepText(step)} averages${SIGNALS[name].columns.length === 1 ? ', shaded from lowest to highest' : ''}`;
</script>

{#if !logbook.canRead('vitals')}
	<p class="text-sm text-muted-foreground">
		Heart rate is part of the vitals, which this logbook doesn’t share with you.
	</p>
{:else}
	<div class="grid gap-6">
		<Card.Root>
			<Card.Header>
				<Card.Title>Heart rate through the day</Card.Title>
				<Card.Description>
					Drag across the chart, or use the buttons, to zoom in. Under 15 minutes you see it second
					by second, plus the ECG and movement below.
				</Card.Description>
			</Card.Header>
			<Card.Content class="grid gap-4">
				<div class="flex flex-wrap items-end gap-2">
					<div class="grid gap-1.5">
						<Label for="hr-day">Day</Label>
						<Input
							id="hr-day"
							type="date"
							max={today}
							class="w-40"
							value={day}
							onchange={(e) => {
								if (!e.currentTarget.value) return;
								day = e.currentTarget.value;
								view = dayBounds(day);
							}}
						/>
					</div>
					<Button variant="outline" onclick={() => zoom(1 / 4)} disabled={span <= MIN_SPAN}
						>Zoom in</Button
					>
					<Button variant="outline" onclick={() => zoom(4)} disabled={span >= bounds[1] - bounds[0]}
						>Zoom out</Button
					>
					<Button variant="outline" onclick={() => pan(-1 / 2)} disabled={view[0] <= bounds[0]}
						>Earlier</Button
					>
					<Button variant="outline" onclick={() => pan(1 / 2)} disabled={view[1] >= bounds[1]}
						>Later</Button
					>
					<Button
						variant="ghost"
						onclick={() => (view = bounds)}
						disabled={span >= bounds[1] - bounds[0]}>Whole day</Button
					>
				</div>
				<p class="text-sm text-muted-foreground tabular-nums" aria-live="polite">
					{span >= bounds[1] - bounds[0] ? 'Whole day' : `${time(view[0])} – ${time(view[1])}`} ·
					{detail('heart_rate')}
				</p>

				{#if hr.isPending}
					<Skeleton class="h-[300px]" />
				{:else if hr.isError}
					<p class="text-sm text-destructive">
						Your heart rate could not be loaded: {hr.error.message}
					</p>
				{:else if hr.data && !hr.data.rows.length && !hr.isPlaceholderData}
					<EmptyState
						title={span >= bounds[1] - bounds[0]
							? 'No heart rate on this day'
							: 'No heart rate in this window'}
						hint="Readings from your watch or chest strap appear here once they sync. Pick another day or zoom out."
					/>
				{:else if hr.data}
					<div class={hr.isPlaceholderData ? 'opacity-60' : ''}>
						<UPlot
							{...chart('heart_rate', hr.data)}
							label="Heart rate in beats per minute, {time(view[0])} to {time(view[1])}"
						/>
					</div>
					{#if hr.data.truncated}<p class="text-sm text-muted-foreground">{TRUNCATED}</p>{/if}
				{/if}
			</Card.Content>
		</Card.Root>

		{#each DETAILS as s (s.name)}
			<Card.Root>
				<Card.Header>
					<Card.Title>{s.title}</Card.Title>
					<Card.Description>
						{span <= DETAIL_UP_TO ? `Same window, ${detail(s.name)}.` : s.hint}
					</Card.Description>
				</Card.Header>
				<Card.Content>
					{#if span > DETAIL_UP_TO}
						<p class="text-sm text-muted-foreground">
							Zoom in to 15 minutes or less to see {s.what}.
						</p>
					{:else if s.q.isPending}
						<Skeleton class="h-[300px]" />
					{:else if s.q.isError}
						<p class="text-sm text-destructive">
							{s.title} could not be loaded: {s.q.error.message}
						</p>
					{:else if !s.q.data.rows.length && !s.q.isPlaceholderData}
						<p class="text-sm text-muted-foreground">No {s.what} in this window. {s.hint}</p>
					{:else}
						<div class={s.q.isPlaceholderData ? 'opacity-60' : ''}>
							<UPlot
								{...chart(s.name, s.q.data)}
								label="{s.title}, {time(view[0])} to {time(view[1])}"
							/>
						</div>
						{#if s.q.data.truncated}<p class="text-sm text-muted-foreground">{TRUNCATED}</p>{/if}
					{/if}
				</Card.Content>
			</Card.Root>
		{/each}
	</div>
{/if}
