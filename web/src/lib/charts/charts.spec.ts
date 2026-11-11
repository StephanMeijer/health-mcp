import { describe, expect, it } from 'vitest';
import { progressLines } from './progress.js';
import { chartTheme, commaColor } from './theme.js';

describe('progressLines', () => {
	it('draws one line per machine and variant, oldest point first', () => {
		const row = (
			period_start: string,
			machine: string | null,
			variant: string | null,
			value = 1
		) => ({
			period_start,
			machine,
			variant,
			value
		});
		expect(
			progressLines([
				row('2026-10-12', null, null, 100),
				row('2026-10-05', null, null, 90),
				row('2026-10-05', 'Technogym', 'wide grip', 50),
				row('2026-10-05', 'Technogym', null, 60)
			])
		).toEqual([
			{
				name: 'No machine noted',
				points: [
					['2026-10-05', 90],
					['2026-10-12', 100]
				]
			},
			{ name: 'Technogym · wide grip', points: [['2026-10-05', 50]] },
			{ name: 'Technogym', points: [['2026-10-05', 60]] }
		]);
	});

	it('draws nothing without rows', () => {
		expect(progressLines([])).toEqual([]);
	});
});

describe('chart theme', () => {
	it('turns space-separated colours into the comma form ECharts parses', () => {
		expect(commaColor(' hsl(158 64% 32%) ')).toBe('hsl(158, 64%, 32%)');
		expect(commaColor('rgb(1, 2, 3)')).toBe('rgb(1, 2, 3)');
		expect(commaColor('#123456')).toBe('#123456');
	});

	it('reads the series colours and text colours from the theme tokens', () => {
		const tokens: Record<string, string> = {
			'--chart-1': 'hsl(1 2% 3%)',
			'--chart-5': 'hsl(5 5% 5%)',
			'--foreground': 'hsl(0 0% 10%)',
			'--muted-foreground': 'hsl(0 0% 40%)'
		};
		const theme = chartTheme((n) => tokens[n] ?? '#000');
		expect(theme.color[0]).toBe('hsl(1, 2%, 3%)');
		expect(theme.color[4]).toBe('hsl(5, 5%, 5%)');
		expect(theme.textStyle.color).toBe('hsl(0, 0%, 10%)');
		expect(theme.valueAxis.axisLabel.color).toBe('hsl(0, 0%, 40%)');
		// The calendar heatmap's scale runs from the strongest series colour down.
		expect(theme.visualMap.color[0]).toBe('hsl(1, 2%, 3%)');
	});
});
