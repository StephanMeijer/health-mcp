import { describe, expect, it } from 'vitest';
import type { Settings } from '#lib/api/results.js';
import { changedSettings, settingsForm } from './settings.js';

const settings: Settings = {
	timezone: 'UTC',
	now: '2026-11-10T13:00:00Z',
	today: '2026-11-10',
	goal: null,
	goals_notes: null,
	priority_muscle_groups: ['chest', 'lats'],
	limitations: null
};

describe('changedSettings', () => {
	it('sends nothing for an untouched form, whatever the muscle order', () => {
		const f = settingsForm(settings);
		f.priority_muscle_groups.reverse();
		f.goals_notes = '  ';
		expect(changedSettings(settings, f)).toEqual({});
	});

	it('clears muscle groups with []', () => {
		expect(
			changedSettings(settings, { ...settingsForm(settings), priority_muscle_groups: [] })
		).toEqual({ priority_muscle_groups: [] });
	});
});
