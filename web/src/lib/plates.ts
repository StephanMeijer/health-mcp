/** Competition plate colours (IWF): the web UI marks a weight with the colour of the plate band it falls in. */
export type Plate = 25 | 20 | 15 | 10 | 5;

/** Upper bounds (exclusive) in kg for each band, lightest first. 120 kg and up is a red 25 kg day. */
const BANDS: [number, Plate][] = [
	[40, 5],
	[60, 10],
	[80, 15],
	[120, 20]
];

/** The plate band for a load in kg; `null` for no load (bodyweight, cardio). */
export function plateFor(kg: number | null | undefined): Plate | null {
	if (kg == null || !(kg > 0)) return null;
	return BANDS.find(([max]) => kg < max)?.[1] ?? 25;
}

// Whole class names, so Tailwind's scanner finds them (it can't see `bg-plate-${plate}`).
const CLASS: Record<Plate, string> = {
	25: 'bg-plate-25',
	20: 'bg-plate-20',
	15: 'bg-plate-15',
	10: 'bg-plate-10',
	5: 'bg-plate-5'
};

/** The Tailwind background class for a plate, from the theme tokens in layout.css. */
export function plateClass(plate: Plate | null): string {
	return plate === null ? 'bg-transparent' : CLASS[plate];
}
