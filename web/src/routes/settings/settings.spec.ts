import { describe, expect, it } from 'vitest';
import type { BodyNote, Settings } from '#lib/api/results.js';
import {
	addNoteArgs,
	changedSettings,
	noteForm,
	settingsForm,
	updateNoteArgs
} from './settings.js';

const settings: Settings = {
	timezone: 'UTC',
	now: '2026-11-10T13:00:00Z',
	today: '2026-11-10',
	goal: null,
	goals_notes: null,
	priority_muscle_groups: ['chest', 'lats'],
	limitations: null
};

const note: BodyNote = {
	id: 7,
	noted_at: '2026-11-09T18:30:00Z',
	location: 'left knee',
	severity: 4,
	notes: 'after lunges',
	workout_id: null
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

describe('body note args', () => {
	it('sends only the id when nothing changed', () => {
		expect(updateNoteArgs(note, noteForm(note))).toEqual({ id: 7 });
	});

	it('clears the severity with 0', () => {
		expect(updateNoteArgs(note, { ...noteForm(note), severity: null })).toEqual({
			id: 7,
			severity: 0
		});
	});

	it('leaves out empty optional fields when adding', () => {
		expect(addNoteArgs({ ...noteForm(), location: ' lower_back ' })).toEqual({
			location: 'lower_back'
		});
	});
});
