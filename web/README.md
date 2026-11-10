# health web app

The browser UI for health-mcp: Svelte 5 + TypeScript (SvelteKit as a single-page app), shadcn-svelte components with
our own theme, built into the server image and served at `/app`.

```sh
corepack enable          # pnpm, in the version package.json pins
pnpm install
pnpm dev                 # http://localhost:5173/app, with /api proxied to a local health-mcp on :8080
pnpm check && pnpm lint && pnpm test && pnpm build
```

- Components: `pnpm dlx shadcn-svelte@latest add <name>` puts them in `src/lib/components/ui` (our code from then on).
  The theme tokens are in `src/routes/layout.css`: pine green for actions, amber for comments, plate colours for weights.
- Tools: `tool(name, args, { owner })` in `src/lib/api/client.ts` calls `/api/v1/tools/{name}` with the session
  cookie. Its argument types are generated from the server: after changing a tool, run
  `UPDATE_TOOLS=1 cargo test --test tools_json` (repo root), then `pnpm gen:tools`. CI fails when either is stale.
- Data: TanStack Query; key queries by `[logbook.owner, tool, args]` so switching logbooks refetches.
- Imports use `#lib/...` with the file named (`#lib/plates.js`, `#lib/components/ui/button/index.js`).
- Claude: the repo's `.mcp.json` adds the official Svelte MCP server (docs lookup and an autofixer for Svelte 5 code).
