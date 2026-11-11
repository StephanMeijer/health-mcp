/** Reads one CSS custom property, e.g. `(n) => getComputedStyle(el).getPropertyValue(n)`. */
export type TokenReader = (name: string) => string;

/**
 * zrender parses only the comma form of `hsl()`/`rgb()`; our tokens (layout.css) use the space form.
 * shortcut: no `/ alpha` support, add it when a token gets one.
 */
export const commaColor = (c: string) => c.trim().replace(/(?<=[^\s(,])\s+(?=[^\s),])/g, ', ');

/** An ECharts theme from the page's theme tokens, so charts follow light and dark like the rest of the app. */
export function chartTheme(read: TokenReader) {
	const t = (name: string) => commaColor(read(`--${name}`));
	const axis = {
		axisLine: { lineStyle: { color: t('border') } },
		axisTick: { lineStyle: { color: t('border') } },
		axisLabel: { color: t('muted-foreground') },
		splitLine: { lineStyle: { color: t('border') } },
		nameTextStyle: { color: t('muted-foreground') }
	};
	return {
		color: [1, 2, 3, 4, 5, 6, 7].map((i) => t(`chart-${i}`)),
		backgroundColor: 'transparent',
		textStyle: { color: t('foreground'), fontFamily: 'Inter Variable, system-ui, sans-serif' },
		legend: { textStyle: { color: t('foreground') } },
		tooltip: {
			backgroundColor: t('popover'),
			borderColor: t('border'),
			textStyle: { color: t('popover-foreground') }
		},
		// Calendar heatmap: empty days as muted squares, more sets darker (visualMap lists high first).
		visualMap: {
			color: [t('chart-1'), t('secondary')],
			textStyle: { color: t('muted-foreground') }
		},
		calendar: {
			itemStyle: { color: t('muted'), borderColor: t('card'), borderWidth: 2 },
			splitLine: { show: false },
			dayLabel: { color: t('muted-foreground') },
			monthLabel: { color: t('muted-foreground') },
			yearLabel: { show: false }
		},
		categoryAxis: axis,
		valueAxis: axis,
		timeAxis: axis
	};
}
