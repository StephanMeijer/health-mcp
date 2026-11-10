<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import { plateClass, plateFor } from '#lib/plates.js';

	// Placeholder until the app shell (#102): shows the theme with a sample of logged sets.
	const sets = [
		{ exercise: 'Back squat', reps: 8, kg: 60 },
		{ exercise: 'Back squat', reps: 5, kg: 100 },
		{ exercise: 'Leg press', reps: 12, kg: 140 }
	];
</script>

<main class="mx-auto grid max-w-xl gap-6 p-6">
	<Card.Root>
		<Card.Header>
			<Card.Title class="text-2xl">Health</Card.Title>
			<Card.Description>The web app is on its way. Example sets show the theme.</Card.Description>
		</Card.Header>
		<Card.Content>
			<ul class="grid gap-2">
				{#each sets as set, i (i)}
					<li class="flex items-center justify-between border-b py-2 last:border-0">
						<span>{set.exercise}</span>
						<span class="flex items-center gap-2 font-numerals text-xl tabular-nums">
							<span class="h-5 w-1.5 rounded-sm {plateClass(plateFor(set.kg))}"></span>
							{set.reps} × {set.kg}
						</span>
					</li>
				{/each}
			</ul>
		</Card.Content>
		<Card.Footer class="gap-2">
			<Button>Add set</Button>
			<Button variant="outline">Open logbook</Button>
		</Card.Footer>
	</Card.Root>
</main>
