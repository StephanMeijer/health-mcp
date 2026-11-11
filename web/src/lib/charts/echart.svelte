<script lang="ts" module>
	import * as echarts from 'echarts/core';
	import { BarChart, HeatmapChart, LineChart, ScatterChart } from 'echarts/charts';
	import {
		CalendarComponent,
		GridComponent,
		LegendComponent,
		MarkAreaComponent,
		MarkPointComponent,
		TooltipComponent,
		VisualMapComponent
	} from 'echarts/components';
	import { CanvasRenderer } from 'echarts/renderers';

	// Tree-shaken: register only what the charts use; add a chart or component here when a page needs it.
	echarts.use([
		BarChart,
		HeatmapChart,
		LineChart,
		ScatterChart,
		CalendarComponent,
		GridComponent,
		LegendComponent,
		MarkAreaComponent,
		MarkPointComponent,
		TooltipComponent,
		VisualMapComponent,
		CanvasRenderer
	]);
</script>

<script lang="ts">
	import { chartTheme, commaColor } from './theme.js';

	/** A theme token's colour by name (`chart-3`), for options that colour marks themselves. */
	type Token = (name: string) => string;

	/**
	 * One ECharts chart: follows `option`, its box size and the light/dark setting. `label` describes it to screen
	 * readers. An `option` function gets the theme tokens and runs again when the theme flips.
	 */
	let {
		option,
		label,
		class: className = 'h-72'
	}: {
		option: echarts.EChartsCoreOption | ((token: Token) => echarts.EChartsCoreOption);
		label: string;
		class?: string;
	} = $props();

	function mount(el: HTMLDivElement) {
		const read = (n: string) => getComputedStyle(el).getPropertyValue(n);
		const theme = () => chartTheme(read);
		const token: Token = (n) => commaColor(read(`--${n}`));
		const chart = echarts.init(el, theme());
		const draw = () => chart.setOption(typeof option === 'function' ? option(token) : option, true);
		$effect(draw);
		const resize = new ResizeObserver(() => chart.resize());
		resize.observe(el);
		// mode-watcher flips the `dark` class on <html> a frame after the setting changes: re-read the tokens then.
		const retheme = new MutationObserver(() => {
			chart.setTheme(theme());
			if (typeof option === 'function') draw();
		});
		retheme.observe(document.documentElement, { attributeFilter: ['class'] });
		return () => {
			resize.disconnect();
			retheme.disconnect();
			chart.dispose();
		};
	}
</script>

<div {@attach mount} role="img" aria-label={label} class={className}></div>
