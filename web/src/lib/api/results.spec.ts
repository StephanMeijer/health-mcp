import { describe, expect, it } from 'vitest';
import { sessions, type SleepSession } from './results.js';

const night = {
	id: 'a',
	source: 'manual',
	start_at: '',
	end_at: '',
	note: null,
	hours: 7.5,
	stage_seconds: null,
	stages: null
} satisfies SleepSession;

describe('sessions', () => {
	it('reads the plain array', () => expect(sessions([night])).toEqual([night]));
	it('reads the capped shape', () =>
		expect(sessions({ truncated: true, sessions: [night] })).toEqual([night]));
	it('reads nothing as no nights', () => expect(sessions(undefined)).toEqual([]));
});
