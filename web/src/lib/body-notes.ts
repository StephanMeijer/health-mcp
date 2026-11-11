import type { BodyNote } from '#lib/api/results.js';
import type { BodyNoteAddArgs } from '#lib/api/tools.gen.js';

/** The body note form's fields. `noted_at` is a `datetime-local` value in the browser's timezone. */
export interface NoteForm {
	location: string;
	severity: number | null;
	noted_at: string;
	notes: string;
}

/** An RFC 3339 time as a `datetime-local` value in the browser's timezone. */
export function localInput(iso: string): string {
	const d = new Date(iso);
	const p = (n: number) => String(n).padStart(2, '0');
	return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function noteForm(n?: BodyNote): NoteForm {
	return {
		location: n?.location ?? '',
		severity: n?.severity ?? null,
		noted_at: n ? localInput(n.noted_at) : '',
		notes: n?.notes ?? ''
	};
}

// An empty number input binds as null or undefined.
const severityOf = (f: NoteForm) => (f.severity == null ? null : f.severity);

/** body_note_add arguments; an empty time means now. */
export function addNoteArgs(f: NoteForm): Omit<BodyNoteAddArgs, 'owner'> {
	return {
		location: f.location.trim(),
		...(severityOf(f) !== null && { severity: severityOf(f) }),
		...(f.noted_at && { noted_at: new Date(f.noted_at).toISOString() }),
		...(f.notes.trim() && { notes: f.notes.trim() })
	};
}

/** body_note_update arguments for what changed; an emptied severity sends 0, which clears it. */
export function updateNoteArgs(n: BodyNote, f: NoteForm) {
	const out: {
		id: number;
		location?: string;
		severity?: number;
		noted_at?: string;
		notes?: string;
	} = { id: n.id };
	if (f.location.trim() !== n.location) out.location = f.location.trim();
	if (severityOf(f) !== n.severity) out.severity = severityOf(f) ?? 0;
	if (f.noted_at && f.noted_at !== localInput(n.noted_at))
		out.noted_at = new Date(f.noted_at).toISOString();
	if (f.notes.trim() !== (n.notes ?? '')) out.notes = f.notes.trim();
	return out;
}
