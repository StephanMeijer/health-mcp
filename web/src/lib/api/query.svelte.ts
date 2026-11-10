import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
import { toast } from 'svelte-sonner';
import { logbook } from '#lib/logbook.svelte.js';
import { tool, ToolUserError } from './client.js';
import { ownerTools, type ToolArgs, type ToolName } from './tools.gen.js';

type Args<N extends ToolName> = Omit<ToolArgs[N], 'owner'>;

/** The cache key of a tool call on a logbook (the current one by default): switching never shows another owner's data. */
export function toolKey<N extends ToolName>(name: N, args?: Args<N>, owner = logbook.owner) {
	return [
		ownerTools.has(name) ? owner : null,
		name,
		...(args === undefined ? [] : [args])
	] as const;
}

/**
 * Reads a tool's result, typed by the caller (`toolQuery<Gym[]>(…)`). `args` is a function so the query follows
 * reactive state (a search box, a selected kind).
 */
export function toolQuery<T, N extends ToolName = ToolName>(
	name: N,
	args: () => Args<N>,
	opts: { enabled?: () => boolean } = {}
) {
	return createQuery(() => {
		const a = args();
		return {
			queryKey: toolKey(name, a),
			queryFn: () => tool(name, a, { owner: logbook.owner }) as Promise<T>,
			enabled: opts.enabled?.() ?? true
		};
	});
}

/**
 * Runs a write tool on the current logbook. On success: a toast (`success`) and a refetch of the listed tools'
 * queries. On a refused request the server's message becomes the toast, so the person sees what to change.
 */
export function toolMutation<T, N extends ToolName = ToolName>(
	name: N,
	opts: { invalidate?: ToolName[]; success?: string | ((result: T) => string) } = {}
) {
	const client = useQueryClient();
	return createMutation(() => ({
		// The logbook written to, kept for the invalidation: it may be switched while the write is in flight.
		onMutate: () => ({ owner: logbook.owner }),
		mutationFn: (args: Args<N>) => tool(name, args, { owner: logbook.owner }) as Promise<T>,
		onSuccess: (result: T, _args: Args<N>, { owner }: { owner: string | null }) => {
			if (opts.success)
				toast.success(typeof opts.success === 'string' ? opts.success : opts.success(result));
			for (const t of opts.invalidate ?? [])
				client.invalidateQueries({ queryKey: toolKey(t, undefined, owner) });
		},
		// A 401 never gets here: tool() sends the browser to the login and never settles.
		onError: (e: Error) => {
			toast.error(
				e instanceof ToolUserError ? e.message : 'That didn’t work. Try again in a moment.'
			);
		}
	}));
}
