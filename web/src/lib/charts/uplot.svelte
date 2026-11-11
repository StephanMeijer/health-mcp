<script lang="ts">
	import uPlot from 'uplot';
	import 'uplot/dist/uPlot.min.css';
	import { uplotTheme, type UplotTheme } from './theme.js';

	/**
	 * One uPlot chart, for series too long for ECharts: follows `options`, `data`, its width and the light/dark
	 * setting. `window` pins the x scale (epoch seconds). `label` describes it to screen readers.
	 */
	let {
		options,
		data,
		window: win,
		label,
		height = 260
	}: {
		options: (theme: UplotTheme) => Omit<uPlot.Options, 'width' | 'height'>;
		data: uPlot.AlignedData;
		window?: [number, number];
		label: string;
		height?: number;
	} = $props();

	function mount(el: HTMLDivElement) {
		const read = () => uplotTheme((n) => getComputedStyle(el).getPropertyValue(n));
		let theme = $state(read());
		// Rebuilt on any change: a chart holds at most ~1000 points, so that is cheaper than diffing options.
		$effect(() => {
			const u = new uPlot({ ...options(theme), width: el.clientWidth, height }, data, el);
			if (win) u.setScale('x', { min: win[0], max: win[1] });
			const resize = new ResizeObserver(() => u.setSize({ width: el.clientWidth, height }));
			resize.observe(el);
			return () => {
				resize.disconnect();
				u.destroy();
			};
		});
		// mode-watcher flips the `dark` class on <html> a frame after the setting changes: re-read the tokens then.
		const retheme = new MutationObserver(() => (theme = read()));
		retheme.observe(document.documentElement, { attributeFilter: ['class'] });
		return () => retheme.disconnect();
	}
</script>

<div {@attach mount} role="img" aria-label={label} class="w-full"></div>
