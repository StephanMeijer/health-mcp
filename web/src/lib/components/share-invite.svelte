<script lang="ts">
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import ShareLevel from '#lib/components/share-level.svelte';
	import { toolMutation } from '#lib/api/query.svelte.js';
	import type { ShareInviteResult } from '#lib/api/results.js';
	import type { Access } from '#lib/logbook.svelte.js';

	let open = $state(false);
	let email = $state('');
	let gym = $state<Access>('none');
	let vitals = $state<Access>('none');
	let problem = $state('');
	let result = $state<ShareInviteResult | null>(null);
	let linkEl = $state<HTMLInputElement | null>(null);

	const invite = toolMutation<ShareInviteResult>('share_invite', {
		invalidate: ['share_list'],
		success: (r) => (r.mailed ? `Invite mailed to ${r.invited}` : `Invite created for ${r.invited}`)
	});

	function reset() {
		email = '';
		gym = vitals = 'none';
		problem = '';
		result = null;
	}

	function submit(e: SubmitEvent) {
		e.preventDefault();
		if (gym === 'none' && vitals === 'none') {
			problem = 'Choose read or read & write for your gym logbook, your vitals, or both.';
			return;
		}
		problem = '';
		invite.mutate({ email: email.trim(), gym, vitals }, { onSuccess: (r) => (result = r) });
	}

	function copy() {
		navigator.clipboard.writeText(result!.link).catch(() => linkEl?.select());
	}
</script>

<!-- Reset on open: Done closes by setting `open`, which skips onOpenChange. -->
<Dialog.Root bind:open onOpenChange={(o) => o && reset()}>
	<Dialog.Trigger>
		{#snippet child({ props })}<Button {...props}>Invite someone</Button>{/snippet}
	</Dialog.Trigger>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>Invite someone</Dialog.Title>
			<Dialog.Description>
				They accept by logging in with this email address. You can change or stop sharing later.
			</Dialog.Description>
		</Dialog.Header>
		{#if result}
			<div class="grid gap-3 text-sm">
				{#if result.mailed}
					<p>
						Mailed to <b>{result.invited}</b>. The invite expires on {new Date(
							result.expires_at
						).toLocaleString()}.
					</p>
				{:else}
					<p>
						No mail server is set up, so send <b>{result.invited}</b> this link yourself. It expires
						on
						{new Date(result.expires_at).toLocaleString()}.
					</p>
					<div class="flex gap-2">
						<Label for="invite-link" class="sr-only">Invite link</Label>
						<Input id="invite-link" readonly value={result.link} bind:ref={linkEl} />
						<Button variant="outline" onclick={copy}>Copy link</Button>
					</div>
				{/if}
			</div>
			<Dialog.Footer><Button onclick={() => (open = false)}>Done</Button></Dialog.Footer>
		{:else}
			<form class="grid gap-4" onsubmit={submit}>
				<div class="grid gap-2">
					<Label for="invite-email">Email</Label>
					<Input id="invite-email" type="email" required autocomplete="off" bind:value={email} />
				</div>
				<div class="flex flex-wrap gap-4">
					<div class="grid gap-2">
						<Label for="invite-gym">Gym logbook</Label>
						<ShareLevel id="invite-gym" label="Gym logbook" bind:value={gym} />
					</div>
					<div class="grid gap-2">
						<Label for="invite-vitals">Vitals</Label>
						<ShareLevel id="invite-vitals" label="Vitals" bind:value={vitals} />
					</div>
				</div>
				{#if problem}<p class="text-sm text-destructive" role="alert">{problem}</p>{/if}
				<Dialog.Footer>
					<Button type="submit" disabled={invite.isPending}>Send invite</Button>
				</Dialog.Footer>
			</form>
		{/if}
	</Dialog.Content>
</Dialog.Root>
