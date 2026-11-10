import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import { kindInfo } from '#lib/kinds.js';
import type { MeasurementRow } from '#lib/api/results.js';
import ReadingTable from './reading-table.svelte';

const row = (time: string, source: MeasurementRow['source']): MeasurementRow => ({
	time,
	source,
	device_id: null,
	type: 'weight',
	value: 80,
	unit: 'kg',
	note: null
});
const rows = [
	row('2026-11-09T07:00:00Z', 'manual'),
	row('2026-11-08T07:00:00Z', 'app'),
	row('2026-11-07T07:00:00Z', 'import')
];
const props = { kind: kindInfo('weight')!, rows, onedit: () => {}, ondelete: () => {} };

describe('ReadingTable', () => {
	it('offers edit and delete only on manual rows', () => {
		render(ReadingTable, { ...props, canWrite: true });
		expect(screen.getAllByRole('button', { name: 'Edit' })).toHaveLength(1);
		expect(screen.getAllByRole('button', { name: 'Delete reading' })).toHaveLength(1);
		expect(screen.getAllByText('Read-only')).toHaveLength(2);
	});

	it('offers nothing to change on a read-only logbook', () => {
		render(ReadingTable, { ...props, canWrite: false });
		expect(screen.queryByRole('button')).toBeNull();
		expect(screen.queryByText('Read-only')).toBeNull();
	});

	it('lists readings that share a time but not a source', () => {
		const time = '2026-11-09T07:00:00Z';
		render(ReadingTable, {
			...props,
			rows: [row(time, 'manual'), row(time, 'app')],
			canWrite: false
		});
		expect(screen.getAllByRole('row')).toHaveLength(3);
	});
});
