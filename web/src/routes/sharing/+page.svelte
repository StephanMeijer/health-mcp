<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import PageHeader from '#lib/components/page-header.svelte';
	import EmptyState from '#lib/components/empty-state.svelte';
	import ConfirmDelete from '#lib/components/confirm-delete.svelte';
	import ShareInvite from '#lib/components/share-invite.svelte';
	import ShareLevel, { LEVELS } from '#lib/components/share-level.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Table from '#lib/components/ui/table/index.js';
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import type { ShareList } from '#lib/api/results.js';
	import { logbook, type Access } from '#lib/logbook.svelte.js';

	const list = toolQuery<ShareList>('share_list', () => ({}));
	const update = toolMutation('share_update', {
		invalidate: ['share_list'],
		success: 'Sharing changed'
	});
	const removed = {
		revoke: (email: string) => `You no longer share with ${email}`,
		leave: (email: string) => `You left the logbook of ${email}`,
		withdraw: () => 'Invite withdrawn'
	};
	const remove = toolMutation<{ email: string; removed: keyof typeof removed }>('share_remove', {
		invalidate: ['share_list'],
		success: (r) => removed[r.removed](r.email)
	});
	const respond = toolMutation<{ accepted: boolean; owner: string }>('share_respond', {
		invalidate: ['share_list'],
		success: (r) => (r.accepted ? `You can now open the logbook of ${r.owner}` : 'Invite declined')
	});

	const date = (s: string) => new Date(s).toLocaleDateString();
	const levels = (s: { gym: Access; vitals: Access }) =>
		`Gym logbook: ${LEVELS[s.gym].toLowerCase()} · Vitals: ${LEVELS[s.vitals].toLowerCase()}`;
	const refresh = () => logbook.refreshMe();

	function open(owner: string) {
		logbook.owner = owner;
		goto(resolve('/workouts'));
	}
</script>

<PageHeader
	title="Sharing"
	description="Who sees your logbook and vitals, and whose you can see. Always about your own account."
>
	{#snippet actions()}
		{#if list.data?.your_email}<ShareInvite />{/if}
	{/snippet}
</PageHeader>

{#if list.isPending}
	<div class="grid gap-3">
		<Skeleton class="h-10 w-full" />
		<Skeleton class="h-10 w-full" />
		<Skeleton class="h-10 w-2/3" />
	</div>
{:else if list.isError}
	<p class="text-sm text-destructive" role="alert">{list.error.message}</p>
{:else}
	{@const d = list.data}
	{#if !d.your_email}
		<p class="mb-6 rounded-md border bg-muted px-3 py-2 text-sm text-muted-foreground">
			Sharing works by email address, and this login has no verified email. Log out and log in again
			with a login that shares your email address.
		</p>
	{/if}

	<div class="grid gap-8">
		{#if d.invites_received.length}
			<section class="grid gap-3">
				<h2 class="text-lg font-semibold">Invites for you</h2>
				{#each d.invites_received as inv (inv.invite_id)}
					<div class="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3">
						<div class="grid gap-0.5">
							<p class="font-medium">{inv.from} wants to share with you</p>
							<p class="text-sm text-muted-foreground">
								{levels(inv)} · expires {date(inv.expires_at)}
							</p>
						</div>
						<div class="flex gap-2">
							<Button
								size="sm"
								disabled={respond.isPending}
								onclick={() =>
									respond.mutate(
										{ invite_id: inv.invite_id, accept: true },
										{ onSuccess: refresh }
									)}>Accept</Button
							>
							<Button
								size="sm"
								variant="ghost"
								disabled={respond.isPending}
								onclick={() => respond.mutate({ invite_id: inv.invite_id, accept: false })}
								>Decline</Button
							>
						</div>
					</div>
				{/each}
			</section>
		{/if}

		<section class="grid gap-3">
			<h2 class="text-lg font-semibold">Shared by you</h2>
			{#if d.shared_by_you.length}
				<Table.Root>
					<Table.Header>
						<Table.Row>
							<Table.Head>Person</Table.Head>
							<Table.Head>Gym logbook</Table.Head>
							<Table.Head>Vitals</Table.Head>
							<Table.Head>Since</Table.Head>
							<Table.Head><span class="sr-only">Actions</span></Table.Head>
						</Table.Row>
					</Table.Header>
					<Table.Body>
						{#each d.shared_by_you as s (s.email)}
							<Table.Row>
								<Table.Cell class="font-medium">{s.email}</Table.Cell>
								<Table.Cell>
									<ShareLevel
										label="Gym logbook for {s.email}"
										value={s.gym}
										onchange={(gym) => update.mutateAsync({ email: s.email, gym })}
									/>
								</Table.Cell>
								<Table.Cell>
									<ShareLevel
										label="Vitals for {s.email}"
										value={s.vitals}
										onchange={(vitals) => update.mutateAsync({ email: s.email, vitals })}
									/>
								</Table.Cell>
								<Table.Cell class="tabular-nums">{date(s.since)}</Table.Cell>
								<Table.Cell class="text-right">
									<ConfirmDelete
										label="Stop sharing"
										description="{s.email} can no longer see your gym logbook or vitals. To share again, send a new invite."
										onconfirm={() => remove.mutate({ email: s.email, what: 'revoke' })}
									/>
								</Table.Cell>
							</Table.Row>
						{/each}
					</Table.Body>
				</Table.Root>
			{:else}
				<EmptyState
					title="You don't share with anyone yet"
					hint="People who accept your invite appear here, and you can change what they see."
				/>
			{/if}
		</section>

		<section class="grid gap-3">
			<h2 class="text-lg font-semibold">Shared with you</h2>
			{#if d.shared_with_you.length}
				{#each d.shared_with_you as s (s.owner)}
					<div class="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3">
						<div class="grid gap-0.5">
							<p class="font-medium">{s.owner}</p>
							<p class="text-sm text-muted-foreground">{levels(s)} · since {date(s.since)}</p>
						</div>
						<div class="flex gap-2">
							<Button size="sm" variant="outline" onclick={() => open(s.owner)}>Open logbook</Button
							>
							<ConfirmDelete
								label="Leave"
								description="You no longer see the data {s.owner} shares with you. They can invite you again."
								onconfirm={() =>
									remove.mutate({ email: s.owner, what: 'leave' }, { onSuccess: refresh })}
							/>
						</div>
					</div>
				{/each}
			{:else}
				<EmptyState
					title="Nobody shares with you yet"
					hint="When someone invites you and you accept, their logbook appears here and in the switcher at the top of the sidebar."
				/>
			{/if}
		</section>

		{#if d.invites_sent.length}
			<section class="grid gap-3">
				<h2 class="text-lg font-semibold">Invites sent</h2>
				{#each d.invites_sent as inv (inv.email)}
					<div class="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3">
						<div class="grid gap-0.5">
							<p class="font-medium">{inv.email}</p>
							<p class="text-sm text-muted-foreground">
								{levels(inv)} · expires {date(inv.expires_at)}
							</p>
						</div>
						<ConfirmDelete
							label="Withdraw invite"
							description="The link in the invite to {inv.email} stops working."
							onconfirm={() => remove.mutate({ email: inv.email, what: 'withdraw' })}
						/>
					</div>
				{/each}
			</section>
		{/if}
	</div>
{/if}
