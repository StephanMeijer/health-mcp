<script lang="ts" module>
	import * as echarts from 'echarts/core';
	import { BarChart, HeatmapChart, LineChart } from 'echarts/charts';
	import {
		CalendarComponent,
		GridComponent,
		LegendComponent,
		TooltipComponent,
		VisualMapComponent
	} from 'echarts/components';
	import { CanvasRenderer } from 'echarts/renderers';

	// Tree-shaken: register only what the charts use; add a chart or component here when a page needs it.
	echarts.use([
		BarChart,
		HeatmapChart,
		LineChart,
		CalendarComponent,
		GridComponent,
		LegendComponent,
		TooltipComponent,
		VisualMapComponent,
		CanvasRenderer
	]);
</script>

<script lang="ts">
	import { chartTheme } from './theme.js';

	/** One ECharts chart: follows `option`, its box size and the light/dark setting. `label` describes it to screen readers. */
	let {
		option,
		label,
		class: className = 'h-72'
	}: { option: echarts.EChartsCoreOption; label: string; class?: string } = $props();

	function mount(el: HTMLDivElement) {
		const theme = () => chartTheme((n) => getComputedStyle(el).getPropertyValue(n));
		const chart = echarts.init(el, theme());
		$effect(() => chart.setOption(option, true));
		const resize = new ResizeObserver(() => chart.resize());
		resize.observe(el);
		// mode-watcher flips the `dark` class on <html> a frame after the setting changes: re-read the tokens then.
		const retheme = new MutationObserver(() => chart.setTheme(theme()));
		retheme.observe(document.documentElement, { attributeFilter: ['class'] });
		return () => {
			resize.disconnect();
			retheme.disconnect();
			chart.dispose();
		};
	}
</script>

<div {@attach mount} role="img" aria-label={label} class={className}></div>
