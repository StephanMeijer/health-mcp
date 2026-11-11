import { describe, expect, it } from 'vitest';
import type { BodyNote } from '#lib/api/results.js';
import { addNoteArgs, noteForm, updateNoteArgs } from './body-notes.js';

const note: BodyNote = {
	id: 7,
	noted_at: '2026-11-09T18:30:00Z',
	location: 'left knee',
	severity: 4,
	notes: 'after lunges',
	workout_id: null
};

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
