import type { Kind } from '#lib/api/tools.gen.js';

/** The reading kinds the vitals tools take (src/tools.rs KINDS, Kind::unit), with the field that holds the value. */
export interface KindInfo {
	kind: Kind;
	label: string;
	field: 'bpm' | 'rmssd_ms' | 'value';
	unit: string;
}

const k = (
	kind: Kind,
	label: string,
	unit: string,
	field: KindInfo['field'] = 'value'
): KindInfo => ({
	kind,
	label,
	field,
	unit
});

export const KINDS: readonly KindInfo[] = [
	k('heart_rate', 'Heart rate', 'bpm', 'bpm'),
	k('hrv', 'HRV (RMSSD)', 'ms', 'rmssd_ms'),
	k('weight', 'Weight', 'kg'),
	k('body_fat', 'Body fat', '%'),
	k('muscle_mass', 'Muscle mass', 'kg'),
	k('bone_mass', 'Bone mass', 'kg'),
	k('body_water', 'Body water', '%'),
	k('visceral_fat', 'Visceral fat', 'level'),
	k('height', 'Height', 'cm'),
	k('waist', 'Waist', 'cm'),
	k('hip', 'Hip', 'cm'),
	k('neck', 'Neck', 'cm'),
	k('temperature', 'Temperature', '°C'),
	k('spo2', 'SpO₂', '%'),
	k('glucose', 'Glucose', 'mmol/L'),
	k('systolic', 'Systolic', 'mmHg'),
	k('diastolic', 'Diastolic', 'mmHg'),
	k('pulse', 'Pulse', 'bpm')
];

export function kindInfo(kind: string): KindInfo | undefined {
	return KINDS.find((x) => x.kind === kind);
}
