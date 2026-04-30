<!-- loswf:plan -->
# Plan #376 v2 — Dashboard CopyAssetModal, AssetDetail wiring, API client + types

**v2 — supersedes [v1](https://github.com/iksnae/kstore/issues/376#issuecomment-4339987888), addresses 1 blocking + 1 minor reviewer gap (poll loop bounds + error handling).**

Changed sections: **Step 3** (poll loop now bounded, with explicit error-handling shape) and a small **Risks** addendum. All other sections — Problem, Approach, Files to touch, New files, Steps 1/2/4, Tests, Validation — are unchanged from v1 and approved by the reviewer.

## Problem
Backend #375 (PR #386 + hotfix #387) deployed `POST /v1/assets/{asset_id}/copy` to dev, but the dashboard still has no UI to invoke it. Owners with multiple configured buckets (`platform` + BYOK r2/b2/wasabi via `external_originals.buckets`) need to launch a copy from `dashboard/src/routes/assets/[asset_id]/+page.svelte` (the page the issue calls "AssetDetail.svelte"), pick a destination, watch the resulting asset transition `pending`/`copying` → `active`/`failed`, and surface the backend's typed error codes.

## Approach
Add typed `copyAsset` to `dashboard/src/lib/api/client.ts` and the `CopyAssetResponse` / `ConfiguredBucket` types to `dashboard/src/lib/api/types.ts`, then ship a thin `CopyAssetModal.svelte` driven by `api.getAccount()` (same pattern at `dashboard/src/routes/account/+page.svelte:266` and `dashboard/src/routes/projects/[project_id]/+page.svelte:60`) — no new backend endpoint needed because `external_originals` is already exposed via `GET /v1/account`. The bucket list is the keys of `external_originals.buckets` (typed `ExternalBucketConfig`, already at `dashboard/src/lib/api/types.ts:240`) plus the synthetic `platform` slot, with the source bucket disabled. After confirm we POST `/v1/assets/{asset_id}/copy`, then poll `api.getAsset(destination_asset_id)` every 2s until `status` is `active` or `failed`, **bounded by a 5-minute wall-clock timeout**. The modal mounts in the page's existing topbar actions row at `dashboard/src/routes/assets/[asset_id]/+page.svelte:104-115`. Modal styling mirrors the dark `font-mono` aesthetic already used in `inspector/Section.svelte` and `routes/assets/+page.svelte` bulk-action snippets.

## Files to touch
- `dashboard/src/lib/api/types.ts` — add `CopyAssetResponse`, `ConfiguredBucket`, `ConfiguredBucketSlot`, `CopyAssetStatus`, `CopyAssetErrorCode`.
- `dashboard/src/lib/api/client.ts` — add `copyAsset(assetId, destinationBucket)` and `listConfiguredBuckets()` helper around `getAccount()`.
- `dashboard/src/routes/assets/[asset_id]/+page.svelte` — import and mount `CopyAssetModal`; add a "Copy to bucket…" button to the topbar actions row at lines 104-115; disable while `asset.status !== 'active'`.

## New files
- `dashboard/src/lib/components/assets/CopyAssetModal.svelte` — destination dropdown, confirm + cancel, bounded poll loop, backend error-code rendering.

## Step-by-step

1. **Add types in `dashboard/src/lib/api/types.ts`** — append:
   - `CopyAssetStatus = 'pending' | 'copying' | 'active' | 'failed' | 'already_exists'` (handler at `src/handlers/copy-asset.ts:212` returns `already_exists` on dedupe).
   - `CopyAssetResponse = { copy_job_id?: string; source_asset_id: string; destination_asset_id: string; destination_bucket: ConfiguredBucketSlot; status: CopyAssetStatus; asset?: Asset; request_id?: string }` (mirrors `src/handlers/copy-asset.ts:211-218, 276-286, 329-339`).
   - `ConfiguredBucketSlot = 'platform' | 'r2' | 'b2' | 'wasabi'` (mirrors `VALID_SLOTS` at `src/handlers/copy-asset.ts:39`).
   - `ConfiguredBucket = { slot: ConfiguredBucketSlot; provider: ExternalBucketProvider | 'platform'; bucket: string; configured: boolean }`.
   - `CopyAssetErrorCode = 'same_bucket' | 'invalid_source_status' | 'bucket_not_configured' | 'tier_limit_exceeded' | 'copy_in_progress' | 'validation_error'`.
   - Outcome: `npx svelte-check` passes.

2. **Add `copyAsset` + `listConfiguredBuckets` to `dashboard/src/lib/api/client.ts`** — between `archiveAsset` (line 194) and `restoreAsset` (line 196):
   - `copyAsset: (id: string, destination_bucket: ConfiguredBucketSlot) => request<CopyAssetResponse>('POST', \`/v1/assets/${id}/copy\`, { destination_bucket })`.
   - `listConfiguredBuckets: async (): Promise<ConfiguredBucket[]>` — calls existing `getAccount()` (line 281), reads `external_originals.buckets` (typed `ExternalOriginalsAccountConfig` at `dashboard/src/lib/api/types.ts:249`), returns `[{ slot: 'platform', provider: 'platform', bucket: 'managed', configured: true }, ...buckets entries with has_credentials=true]`.
   - Outcome: typed callers, `dashboard-typecheck` green.

3. **Create `dashboard/src/lib/components/assets/CopyAssetModal.svelte`** — Svelte 5 runes (matches surrounding `AssetCard.svelte` style):
   - Props: `{ asset: Asset; open: boolean; onclose: () => void }`.
   - On `open` flipping true → call `api.listConfiguredBuckets()`. Derive source slot by matching `asset.original_reference?.bucket` against each `ConfiguredBucket.bucket`; absence implies `platform`. That row is rendered disabled.
   - States: `idle` → `submitting` → `polling` (after 200/202 with non-terminal status) → `done` (active or already_exists) → `failed` → `timeout` (new in v2 — see below).
   - Confirm handler: `await api.copyAsset(asset.asset_id, slot)`. If `status === 'active'` or `status === 'already_exists'` → toast then `goto('/assets/' + destination_asset_id)`. Otherwise capture `destination_asset_id` and begin polling.
   - **Bounded poll loop (v2 — addresses GAP 1):**
     - Track `pollStartedAt = Date.now()` and a constant `POLL_TIMEOUT_MS = 5 * 60 * 1000` (5 minutes).
     - Track `pollIntervalMs = 2000`; bump to `5000` after the first observed `429 rate_limited` (be polite under contention) and never reset back down for the lifetime of this modal session.
     - Use `setInterval(tick, pollIntervalMs)` where `tick` is the async function below; on the 429-bump, call `stopPolling()` then re-arm `setInterval` with the new interval.
     - **On every tick**, before the API call, check `Date.now() - pollStartedAt >= POLL_TIMEOUT_MS`. If true → call `stopPolling()`, set state to `timeout`, render in-modal copy: *"Copy is still in progress — check the assets list to see when it lands."* with a button labelled "Go to assets" wired to `goto('/assets')`, and emit a toast with the same message + link. Do **not** auto-close. The user-facing close button still works.
     - The destination asset_id is already known from the POST response, so the user can also navigate to `/assets/{destination_asset_id}` directly via a secondary "View destination" link rendered in the timeout state.
   - **Poll-loop error handling shape (v2 — addresses GAP 2).** The `tick` body MUST follow this exact pattern (builders forget poll-loop error handling without the explicit shape):
     ```ts
     async function tick() {
       if (Date.now() - pollStartedAt >= POLL_TIMEOUT_MS) {
         stopPolling();
         state = 'timeout';
         return;
       }
       try {
         const fresh = await api.getAsset(destinationAssetId);
         if (fresh.status === 'active') {
           stopPolling();
           state = 'done';
           goto('/assets/' + destinationAssetId);
           return;
         }
         if (fresh.status === 'failed') {
           stopPolling();
           state = 'failed';
           failureMessage = 'Copy failed. The destination asset is in a failed state.';
           return;
         }
         // pending | copying — keep polling
       } catch (err) {
         if (err instanceof KstoreApiError) {
           // Transient: keep polling. Do NOT stopPolling, do NOT toast.
           if (err.status === 429) {
             if (pollIntervalMs === 2000) {
               pollIntervalMs = 5000;
               stopPolling();
               pollTimer = setInterval(tick, pollIntervalMs);
             }
             return;
           }
           if (err.status >= 500 && err.status < 600) {
             return; // transient server error, keep polling
           }
           // Any other 4xx (404 destination evaporated, 401 auth expired, 403, etc.):
           // terminal — stop and surface.
           stopPolling();
           state = 'failed';
           failureMessage = err.message;
           return;
         }
         // Non-API error (network blip, AbortError, etc.): keep polling silently.
         return;
       }
     }
     ```
     `stopPolling()` is idempotent: `if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }`. Called from the timeout branch, both terminal branches, the 4xx branch, the modal `onclose`, and `onDestroy`.
   - Error rendering map for the **synchronous** `KstoreApiError.code` thrown from `api.copyAsset(...)` (the POST, not the poll):
     - `same_bucket` → "Pick a different bucket — that's where the asset already lives."
     - `invalid_source_status` → "Asset must be active to copy. Current status: {status}."
     - `bucket_not_configured` → "That bucket isn't configured for your account yet."
     - `tier_limit_exceeded` → "Copying would exceed your plan's tier limit. See Account → Plan."
     - `copy_in_progress` → "A copy is already running for this asset." (link to in-flight `destination_asset_id` from `error.details`).
     - default → `error.message`.
   - Tailwind: dark `bg-[#0a0a0a]/90` overlay + `bg-[#0d0d0d]` panel + `font-mono` mirroring `routes/assets/+page.svelte` snippets at lines 179+.
   - Outcome: file exists, `dashboard-typecheck` and `dashboard-build` pass.

4. **Wire from `dashboard/src/routes/assets/[asset_id]/+page.svelte`** — between line 113 (`<Copy>` icon button) and line 115 (closing `</div>`) add a new button using the lucide `ArrowRightLeft` (or `Send`) icon styled identically to the Copy ID button, `aria-label="Copy to another bucket"`, `title="Copy to bucket…"`. State `let copyOpen = $state(false)`; click sets `true`. Render `<CopyAssetModal asset={asset!} open={copyOpen} onclose={() => copyOpen = false} />` outside the `<main>` but inside the root `<div>` so it overlays. Disable the trigger while `loading || !asset || asset.status !== 'active'` (matches backend's `invalid_source_status` precondition).
   - Outcome: clicking the new icon opens the modal; `dashboard-build` succeeds.

## Tests
- No vitest infra in `dashboard/` (`dashboard/package.json` declares only `dev|build|preview|prepare|check|codegen:api`). Acceptance per the issue is `dashboard-typecheck` + `dashboard-build`. Manual smoke against dev API:
  - Copy small (<5 MB) asset from `platform` to a configured `r2` bucket → modal flips to `done` within ~3s and navigates.
  - Copy >100 MB asset → modal stays in `polling`; if SFN runs >5 min, modal flips to `timeout` state with the "Go to assets" CTA and a toast.
  - Force a 5xx during polling (e.g. block the dev API briefly) → modal remains in `polling`, no toast spam, recovers when API returns.
  - Force a 429 during polling → interval bumps to 5s, polling continues silently.
  - `same_bucket` precondition → source row disabled, never submits.
  - `tier_limit_exceeded` against capped account → readable error in `idle`/`failed` rendering.

## Validation
From `.loswf/config.yaml`:
- `dashboard-install` → `cd dashboard && npm ci --prefer-offline`
- `dashboard-typecheck` → `cd dashboard && npx svelte-check --tsconfig ./tsconfig.json`
- `dashboard-build` → `cd dashboard && npm run build`

## Risks
- **Source-slot inference**: `asset.original_reference?.bucket` is the underlying bucket *name*, not the slot key. The `ExternalOriginalsAccountConfig.buckets` map keys are the slot names; each `ExternalBucketConfig.bucket` field is the underlying bucket. Match by `entry.bucket === asset.original_reference?.bucket`. Fall back to `platform` if no match. Documented inline.
- **`already_exists` status**: backend returns this via 200 with `status: 'already_exists'` (`src/handlers/copy-asset.ts:211-218`). Our type union must include it — the issue body's narrower union is incomplete; we extend it (additive).
- **No `aws_s3` BYOK yet**: handler returns 422 `bucket_not_configured` for `aws_s3` (`src/handlers/copy-asset.ts:16-18`). The modal surfaces the error message verbatim; no special-case.
- **Account fetch on every modal open**: small payload (<2 KB), rare action. Acceptable; can be cached later.
- **Polling cleanup**: `setInterval` MUST be cleared on every exit path. `stopPolling()` is idempotent; called from the timeout branch, terminal branches, the 4xx branch, modal `onclose`, and `onDestroy`. Re-arming on the 429-bump goes through `stopPolling()` first to avoid double-timers.
- **5-minute timeout is a UX bound, not a backend SLA**: the SFN may legitimately keep running past 5 minutes for very large copies. The timeout state is explicitly worded as "still in progress" — not "failed" — and surfaces a path back to the assets list.
- **No vitest in dashboard**: dashboard tests are out of scope per the issue ("no new svelte test infra is required"). The factory's `dashboard-*` gates are the contract.
