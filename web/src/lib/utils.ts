export { cn } from 'cn';

// Helper types the shadcn-svelte components import (what `shadcn-svelte init` would generate).
/* eslint-disable @typescript-eslint/no-explicit-any */
export type WithoutChild<T> = T extends { child?: any } ? Omit<T, 'child'> : T;
export type WithoutChildren<T> = T extends { children?: any } ? Omit<T, 'children'> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };

/** "ndsm, trainmore noord" → ["ndsm", "trainmore noord"]: the comma-separated alias inputs. */
export const splitList = (s: string): string[] =>
	s
		.split(',')
		.map((x) => x.trim())
		.filter(Boolean);
