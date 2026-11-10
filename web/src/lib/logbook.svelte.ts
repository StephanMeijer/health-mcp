import { meUrl } from '#lib/server-routes.js';

/** Whose logbook the web app shows: your own, or one someone shared with you (passed to tools as `owner`). */

export type Access = 'none' | 'ro' | 'rw';
export type Domain = 'gym' | 'vitals';

export interface Shared {
	owner: string;
	gym: Access;
	vitals: Access;
}

/** `GET /app/me`. */
export interface Me {
	sub: string;
	email: string | null;
	shared_with_you: Shared[];
}

/** What you may do in `domain` of the logbook of `owner` (`null`: your own). */
export function access(owner: string | null, shared: Shared[], domain: Domain): Access {
	if (owner === null) return 'rw';
	return shared.find((s) => s.owner === owner)?.[domain] ?? 'none';
}

class Logbook {
	me = $state<Me | null>(null);
	/** `null` is your own logbook. shortcut: not remembered across reloads; add localStorage if switching gets tedious. */
	owner = $state<string | null>(null);

	access(domain: Domain): Access {
		return access(this.owner, this.me?.shared_with_you ?? [], domain);
	}

	canWrite(domain: Domain): boolean {
		return this.access(domain) === 'rw';
	}

	canRead(domain: Domain): boolean {
		return this.access(domain) !== 'none';
	}

	/** Refetches `me` after a share changed, so the logbook switcher follows; drops a logbook no longer shared. */
	async refreshMe(): Promise<void> {
		const res = await fetch(meUrl, { credentials: 'same-origin' });
		// shortcut: a failed refetch keeps the old list until the next navigation reloads `me`.
		if (!res.ok) return;
		const me = (await res.json()) as Me;
		this.me = me;
		if (this.owner !== null && !me.shared_with_you.some((s) => s.owner === this.owner))
			this.owner = null;
	}
}

export const logbook = new Logbook();
