import { describe, expect, it } from 'vitest';
import tools from './api/tools.json';
import { KINDS } from './kinds.js';
import { COARSE_GROUPS, MUSCLE_GROUPS } from './muscles.js';

type Def = { enum?: string[] };
const defs = (name: string) =>
	(tools as { name: string; inputSchema: { $defs?: Record<string, Def> } }[]).find(
		(t) => t.name === name
	)!.inputSchema.$defs ?? {};

describe('hand-written lists', () => {
	// tools.json is kept current by the Rust test tests/tools_json.rs, so a new server kind fails here.
	it('reading kinds match the server', () => {
		expect(new Set(defs('reading_add').Kind?.enum)).toEqual(new Set(KINDS.map((k) => k.kind)));
	});

	// Muscle groups are plain strings in the schema (the server validates them), so check the list itself.
	it('coarse names expand to stored muscle groups only', () => {
		for (const [coarse, fine] of Object.entries(COARSE_GROUPS)) {
			expect(MUSCLE_GROUPS as readonly string[], coarse).not.toContain(coarse);
			for (const m of fine) expect(MUSCLE_GROUPS, m).toContain(m);
		}
		expect(new Set(MUSCLE_GROUPS).size).toBe(MUSCLE_GROUPS.length);
	});
});
