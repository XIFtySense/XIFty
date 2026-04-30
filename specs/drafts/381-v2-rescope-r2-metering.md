<!-- loswf:plan -->
# Plan #381 v2: Rescope — B2→R2 managed default + metering-only F + pre-PR hotfix

## Context — what changed since v1
v1 plan rejected for six blockers. This v2 is anchored to the working tree at `2d254192` (fresh clone at `/tmp/kstore-381-replan`, all migration files materialised). Most importantly, the verification step requested by the reviewer surfaced a **pre-existing bug** that is NOT a v1 plan defect — it is a routing gap shipped by #393 (the bulk-migrate worker landed in `56c52e64`). It must be fixed BEFORE PR I or PR I has nothing to swap to. See the new "PR 0 — hotfix" section below.

## Pre-existing bug confirmed (BLOCKER 2 finding — not a fold-in, surfaces as PR 0)

The reviewer's hypothesis is **confirmed by reading the actual files**:

1. `src/core/swap-asset-reference.ts:97-107` writes `kind: 'kstore_managed', provider: 'aws_s3', bucket: <MANAGED_B2_BUCKET>, key: <managed key>` and updates `storage_key` to the new managed key.
2. `src/handlers/migration-worker.ts:264-273` then hard-deletes the source from MediaBucket (`platformStorage.deleteObject(sourceKey)`).
3. On a subsequent download, `src/handlers/get-signed-url.ts:91-93` calls `signExternalOriginal`. That function (`src/lib/external-original-signer.ts:78-79`) short-circuits on `!ref || ref.kind !== 'external'` — so for a `kind === 'kstore_managed'` row it returns `undefined`.
4. `get-signed-url.ts:98-100` falls through to `storage.getDownloadUrl(storageKey, ...)`, where `storage = new StorageService(process.env.MEDIA_BUCKET!)` (line 13). That signs against MediaBucket using the new (managed) `storage_key`.
5. The object is no longer in MediaBucket — it was hard-deleted in step 2.

**Conclusion: every #393-migrated asset 404s on download today.**

The OTHER read path in `complete-upload.ts:99-104` — `byokStorageForAsset` — DOES correctly disambiguate `MANAGED_B2_BUCKET → 'managed'` slot. But that helper is used for COPY/upload-completion paths, not for `get-signed-url`. The two read paths diverge.

This must be fixed BEFORE the env rename (PR I), otherwise we are renaming env vars on top of broken routing.

**ACTION FOR USER (escalation, blocking PR 0):** confirm whether #393 ever ran in a deployed stage. If it did, the on-disk migrated rows are already broken. If it did not (i.e. `MANAGED_DEFAULT_ENABLED !== 'true'` in every stage so far), PR 0 is a forward-fix only — no data recovery needed. Either way PR 0 is required before PR I.

---

# PR 0 — hotfix: route `kstore_managed` originals through the managed-slot signer (lands first)

## Problem
`signExternalOriginal` ignores `kind === 'kstore_managed'` rows. After the swap+delete in #393, the only signer reachable from `get-signed-url.ts` is `storage.getDownloadUrl` against MediaBucket — but the object is gone. Result: 404 on every migrated original.

## Approach
Add a `kstore_managed` dispatch branch in `signExternalOriginal` (mirroring the `aws_s3` and BYOK branches) that resolves the `'managed'` synthesised slot from `getBucketConfig(db, ownerId, 'managed')` and signs through `S3CompatibleStorage`. The `'managed'` slot already exists (`src/core/external-originals-config.ts:326-353`, `synthesiseManagedSlot`) and is driven by `MANAGED_B2_*` env vars. Fix the routing layer that is actually called from `get-signed-url`. Do NOT touch env naming yet — that is PR I.

This branch must execute BEFORE the existing `if (!ref || ref.kind !== 'external')` short-circuit at line 79. Tested against `byokStorageForAsset` in `complete-upload.ts:99-104`, which already does the equivalent disambiguation by env-var match (good prior art).

## Files to touch
- `src/lib/external-original-signer.ts` — add a `kstore_managed` branch BEFORE line 79's `kind !== 'external'` short-circuit. Match pattern from `complete-upload.ts:99-104` (resolve via `getBucketConfig(db, ownerId, 'managed')`, fall back to `undefined` on missing slot — caller treats undefined as "fall through to MediaBucket" which is now safe to revert to a 404 since that IS the broken path).
- `src/handlers/get-signed-url.ts:122-136` — fix the cross-owner `bandwidth_bytes` bug at the same time (gate on `&& isOwner` to mirror line 112). This is the v1 plan's accepted N2 finding; folding it into PR 0 because the same hot path is being modified.

## New files
- *(none)*

## Step-by-step
1. **Add `kstore_managed` branch** in `signExternalOriginal` — matches the structure of the `aws_s3` branch (lines 81-83) but routes through `getBucketConfig + S3CompatibleStorage` (lines 88-97). New unit test: a row with `kind: 'kstore_managed', provider: 'aws_s3', bucket: <MANAGED_B2_BUCKET>` returns a signed URL pointing at the managed-slot endpoint with the row's `key`. *Verifiable: new test `signExternalOriginal — kstore_managed routes to managed slot` asserts the URL host matches the managed endpoint.*
2. **Cross-owner bandwidth gate** in `get-signed-url.ts:122` — change `if (!variant)` to `if (!variant && isOwner)`. *Verifiable: extend the bandwidth-bump test to assert no DDB UPDATE is issued on the cross-owner branch (`isOwner === false`).*

## Tests
- `tests/unit/lib/external-original-signer.test.ts` — add `kstore_managed` cases: (a) routes through managed slot when `getBucketConfig('managed')` returns a config, (b) returns undefined when no managed slot exists (no opt-in), (c) does NOT fall through to provider-keyed BYOK lookup (it must use the named `'managed'` slot, not `bucketNameForProvider('aws_s3')` which is `undefined`).
- `tests/unit/handlers/get-signed-url.test.ts` — add cross-owner read scenario asserting `bandwidth_bytes` is NOT bumped.

## Validation
- `npm ci --prefer-offline`
- `npx tsc --noEmit`
- `npx vitest run`
- `sam build`

## Risks
- **Existing migrated owners with the bug already in prod**: if #393 has run in any deployed stage, those owners' downloads have been failing since the migration. PR 0 unblocks them on next deploy — no data backfill needed; the bytes are still in `MANAGED_B2_BUCKET`, only the signer was wrong.
- **Test coverage gap**: the existing `external-original-signer.test.ts` covers `aws_s3` and BYOK but not `kstore_managed` — that's how this slipped through #393's review. The new tests close the gap.

---

# PR I — B2→R2 managed default swap (depends on PR 0)

## Problem
The synthesised `'managed'` slot in `src/core/external-originals-config.ts:326-353` hard-codes `provider: 'backblaze_b2'` and reads `MANAGED_B2_*` env vars. After PR 0, the read path is correct but still routes to B2. R2 (zero retail egress) replaces managed B2 to fix the $15/TB plan economics.

## Approach
Mechanical sweep: rename `MANAGED_B2_*` → `MANAGED_R2_*`, flip the synth-slot provider to `'cloudflare_r2'`, set `force_path_style: true` (see Design Q below — single value, no contradiction), rename SAM parameters and SSM paths. Add a parallel `MANAGED_LEGACY_B2_*` synthesised slot so the rows already migrated by #393 (which carry the B2 bucket name) keep resolving — the dispatch branch added in PR 0 picks the slot by `ref.bucket` match.

**Design Q1 — env naming.** `MANAGED_R2_*` over generic `MANAGED_*`. Mirrors `bucketNameForProvider` in `src/lib/s3-compatible-client-builder.ts:23-27`; future-proof for managed-Wasabi/managed-AWS distinct namespaces.

**Design Q5 — force_path_style is `true` for R2 (single definitive value).** Mirrors:
- `external-originals-config.ts:298` — v1→v2 R2 BYOK migration uses `true`
- `external-originals-config.ts:27` (doc comment) — `r2: { force_path_style: true }`
- `s3-compatible-storage.ts:59` — defaults `forcePathStyle: true`
The reviewer's blocker 5 is correct: virtual-hosted style only works on `*.r2.cloudflarestorage.com`; path-style works for both vanity endpoints and the standard R2 endpoint. Use `true` for compatibility with the existing R2 BYOK plumbing.

**Design Q6 — legacy-B2 read slot routing.** PR 0 added the `kstore_managed` branch in `signExternalOriginal`. That branch resolves via `getBucketConfig(db, ownerId, 'managed')`. PR I extends it to ALSO try `'managed_legacy_b2'` when `ref.bucket` matches `MANAGED_LEGACY_B2_BUCKET`. The dispatch is by the row's `bucket` field, NOT by `provider` (which is always `'aws_s3'` per `swap-asset-reference.ts:99-100`). This is the explicit fix for reviewer blocker 2's "cannot route by provider" point.

## Files to touch
- `src/core/external-originals-config.ts` — rename env reads at lines 208, 209, 332, 333, 336; flip synth-slot provider at line 338 to `'cloudflare_r2'`; KEEP `force_path_style: true` at line 342 (no flip — see Q5); update doc comments at lines 70-78, 312-323; add a second synth slot `'managed_legacy_b2'` reading `MANAGED_LEGACY_B2_BUCKET`/`*_ENDPOINT`/`*_REGION`/`*_ACCESS_KEY_ID`/`*_SECRET_ACCESS_KEY` and emitting `provider: 'backblaze_b2', force_path_style: true`. Skipped when the legacy-bucket env is unset.
- `src/lib/external-original-signer.ts` — extend the PR 0 `kstore_managed` branch to disambiguate by `ref.bucket`: try `'managed'` first, then `'managed_legacy_b2'` when `ref.bucket === process.env.MANAGED_LEGACY_B2_BUCKET`. Same shape as the dispatch in `complete-upload.ts:99-104`.
- `src/handlers/complete-upload.ts:99-104` — rename env to `MANAGED_R2_BUCKET`; extend slot-disambiguation to ALSO map `ref.bucket === MANAGED_LEGACY_B2_BUCKET → 'managed_legacy_b2'`.
- `src/handlers/migration-worker.ts:144-159` — rename `MANAGED_B2_BUCKET` → `MANAGED_R2_BUCKET`; the already-migrated short-circuit at line 158 must check both the new R2 bucket AND `MANAGED_LEGACY_B2_BUCKET` (skip if matches either) so a re-run does not re-migrate B2-stamped rows.
- `src/handlers/migration-verify.ts:249-251` — rename env, update error message string.
- `src/handlers/migration-enqueue.ts` — comment-only references to "managed B2" at lines 8, 22-26 — generalise to "managed R2".
- `src/core/copy-asset-orchestrator.ts:90,100` — **(BLOCKER 3 — explicit named sub-step in step-by-step below)** rename docstring; change `sourceSlotForAsset` to recognise BOTH `process.env.MANAGED_R2_BUCKET` AND `process.env.MANAGED_LEGACY_B2_BUCKET` as the `'managed'` slot (legacy maps to `'managed_legacy_b2'`).
- `src/core/swap-asset-reference.ts:36,90` — rename docstring `MANAGED_B2_BUCKET` → `MANAGED_R2_BUCKET` (the `managedBucket` field stays generic).
- `template.yaml` — Globals env block at lines 30-35 (`MANAGED_B2_*` → `MANAGED_R2_*`, SSM paths `/kstore/${Stage}/managed-r2/*`); SAM parameter description at lines 42-52 (rewrite "B2" prose to "R2"); SSM resources `ManagedB2AccessKeyIdParam` / `ManagedB2SecretAccessKeyParam` at lines 114-137 → `ManagedR2*`; IAM `ParameterName` glob at line 2440 (and equivalent) → `kstore/${Stage}/managed-r2/*`. Add new SAM params + Globals entries for `MANAGED_LEGACY_B2_*` (default empty strings) + a second IAM statement for `kstore/${Stage}/managed-legacy-b2/*`. Comment at line 1250 — generalise.
- `tests/unit/core/external-originals-config.test.ts:332-460` — rename keys; assert `provider: 'cloudflare_r2'`, `force_path_style: true`. Add four new cases for the `'managed_legacy_b2'` slot.
- `tests/unit/handlers/complete-upload.test.ts:415-416` — rename stubs; add a legacy-bucket scenario asserting routing to `'managed_legacy_b2'`.
- `tests/unit/lib/external-original-signer.test.ts` — extend the PR 0 `kstore_managed` cases with a legacy-bucket variant.
- `tests/unit/handlers/migration-verify.test.ts:147,340-341` — rename env stubs.
- `tests/unit/handlers/migration-worker.test.ts:119-120,177` — rename env stubs; the "already migrated short-circuit" assertion now fires on EITHER bucket.
- `tests/unit/handlers/migration-enqueue.test.ts` — comment-only updates if any (docstrings).
- `tests/unit/handlers/copy-asset.test.ts:74` — rename env stub.
- `tests/integration/upload-fanout.test.ts:16-17,120-121` — rename env stubs (file is at `tests/integration/`, confirmed in tree).
- `docs/ops/managed-b2-cloudflare.md` — supersede with `docs/ops/managed-r2.md` runbook (see New files).

## New files
- `docs/ops/managed-r2.md` — operator runbook: R2 account/API token creation, SSM put-parameter commands for `/kstore/<stage>/managed-r2/access-key-id|secret-access-key`, `MANAGED_DEFAULT_ENABLED` flip drill, legacy-B2 read-slot note, `delete-parameter` cleanup for the orphaned `/managed-b2/*` SSM params (addresses N1).

## Step-by-step
1. **Add legacy-B2 read slot** in `synthesiseManagedSlot` — synthesise `'managed_legacy_b2'` when `MANAGED_LEGACY_B2_BUCKET` is set; provider `backblaze_b2`, force_path_style `true`. *Verifiable: new test `synthesiseManagedSlot — legacy B2 slot` asserts shape and absence when env unset.*
2. **Flip synth slot to R2** in `synthesiseManagedSlot` — provider `'cloudflare_r2'`, `force_path_style` STAYS `true` (no change at line 342), env reads renamed to `MANAGED_R2_*`. *Verifiable: existing synth-slot tests updated, all green.*
3. **Update read-path disambiguation in `signExternalOriginal`** (the PR 0 branch) — when `ref.kind === 'kstore_managed'`, prefer `'managed_legacy_b2'` when `ref.bucket === process.env.MANAGED_LEGACY_B2_BUCKET`, else fall back to `'managed'`. *Verifiable: new test in `external-original-signer.test.ts` covers a row with the legacy bucket name.*
4. **Update read-path disambiguation in `complete-upload.ts:99-104`** — same disambiguation, mirrored. *Verifiable: new test in `complete-upload.test.ts` covers the legacy-bucket scenario.*
5. **(BLOCKER 3 explicit named sub-step)** **Update `sourceSlotForAsset` in `copy-asset-orchestrator.ts:95-109`** — change the existing `ref.bucket === process.env.MANAGED_B2_BUCKET` check at line 100 to a named helper `isManagedSlotBucket(ref.bucket)` that returns true when `ref.bucket === process.env.MANAGED_R2_BUCKET || ref.bucket === process.env.MANAGED_LEGACY_B2_BUCKET`. The slot returned stays `'managed'` for both — the orchestrator does not need to distinguish (it only uses the slot to resolve a client; both clients can read). *Verifiable: extend `tests/unit/handlers/copy-asset.test.ts` with a legacy-bucket source-slot detection case.*
6. **Update migration-worker short-circuit at `migration-worker.ts:155-162`** — the `bucket === managedBucket` test must accept either env (R2 or legacy-B2). Use the same `isManagedSlotBucket` helper from step 5 (or duplicate the disjunction inline since the helper lives in copy-asset-orchestrator). *Verifiable: extend the migration-worker test for both bucket names.*
7. **SAM template rename** — Globals, SSM resources, IAM `ParameterName` glob, plus new `MANAGED_LEGACY_B2_*` params/Globals/IAM. Run `sam validate` and `sam build`. *Verifiable: validate + build green.*
8. **Runbook** — `docs/ops/managed-r2.md` with the four `aws ssm put-parameter` commands (R2 new, legacy-B2 copy from old path) and a final `aws ssm delete-parameter` for `/kstore/<stage>/managed-b2/*` once the IAM policy no longer references it. *Verifiable: doc lints clean (markdownlint if configured); reviewer reads.*
9. **Sweep all docstrings** referencing "managed B2" — generalise to "managed R2" with a parenthetical "(legacy B2 reads continue via the `managed_legacy_b2` slot)" where relevant.

## Tests
- `tests/unit/core/external-originals-config.test.ts` — extend five existing synth-slot scenarios with R2 expectations + four new legacy-B2 cases (set/unset, default-bucket interaction, credential resolution, no collision with R2 slot).
- `tests/unit/lib/external-original-signer.test.ts` — extend PR 0 `kstore_managed` cases with a legacy-bucket variant resolving through `'managed_legacy_b2'`.
- `tests/unit/handlers/complete-upload.test.ts` — legacy-bucket download scenario (asset row has `original_reference.bucket = MANAGED_LEGACY_B2_BUCKET`).
- `tests/unit/handlers/migration-worker.test.ts` — already-migrated short-circuit fires on either bucket name.
- `tests/unit/handlers/copy-asset.test.ts` — `sourceSlotForAsset` legacy-bucket case.
- All other listed test files: mechanical env-name rename only.

## Validation
- `npm ci --prefer-offline`
- `npx tsc --noEmit`
- `npx vitest run`
- `sam build`
- `cd dashboard && npm ci --prefer-offline && npx svelte-check --tsconfig ./tsconfig.json && npm run build`

## Risks
- **Already-migrated #393 data**: PR 0 already routes `kstore_managed` correctly through the managed slot. PR I's legacy slot keeps that working AFTER the env rename. Step 1 lands the slot; step 3 lands the dispatch — both must be in the same PR.
- **SSM rename under `DeletionPolicy: Retain`**: the runbook (step 8) explicitly lists `delete-parameter` cleanup. Without it, the old `/managed-b2/*` params orphan.

---

# PR F (rescoped) — metering + hard-cap (depends on PR I)

## Problem
Sub-PR F was originally Stripe-integrated overage billing. Rescoped to ship only what is useful for one customer: monthly egress counter + 429 hard-cap + dashboard surface. Drop Stripe.

The existing `bandwidth_bytes` counter (`get-signed-url.ts:122-136`) is cumulative since account creation. The dashboard `+page.svelte:58-62` already declares `egress_used_bytes`/`egress_quota_bytes`/`plan_tb_cap_bytes` reads from `/v1/billing/status` — but `billing-status.ts` does not emit them.

`tier-guard.ts` at HEAD uses a hardcoded `LEGACY_MAX_TIER_BYTES = 2_000 * GB` (line 32), NOT a per-account `plan_tb_cap_bytes`. There is no `plan_tb_cap_bytes` field anywhere on the ACCOUNT row at HEAD (verified by `grep -rn "plan_tb"` on `/tmp/kstore-381-replan/src` — zero hits). #380 never landed the field. This means F itself must add `plan_tb_cap_bytes` to the ACCOUNT row, with a default, before the cap math becomes meaningful.

## Approach
Add `monthly_egress_bytes` on a sibling DDB row keyed `OWNER#<id>` / `USAGE#<YYYY-MM>` (Design Q3 — TTL-based reset, no cron). Meter at signed-URL issuance (Design Q2 — already in the hot path). Pre-issue check returns 429 with `code: egress_cap_exceeded` when projected egress would exceed `2 * plan_tb_cap_bytes`.

**Design Q4 — egress quota source must match across `tier-guard.ts` and `billing-status.ts` (BLOCKER 6 fix).** Both read the SAME field from the ACCOUNT row: `plan_tb_cap_bytes`. F adds the field with a step (see step 1 below) and a constant fallback `DEFAULT_PLAN_TB_CAP_BYTES = 1_000_000_000_000` (1 TB) for any pre-existing ACCOUNT row that doesn't have the field. Both consumers compute `egress_quota_bytes = 2 * planTbCapBytes`. Single helper `getPlanTbCapBytes(account)` lives in `src/lib/billing-plan.ts` (new file) and is imported by both `checkEgressCap` in `tier-guard.ts` and `billing-status.ts`.

**Design Q7 — TTL attribute name (BLOCKER 4 fix).** `template.yaml:375-377` defines `AttributeName: ttl` as the table TTL. SWEEPLOG rows (#394) already use `ttl`. The new monthly USAGE row MUST use `ttl` (Unix seconds) as its field name. NO `template.yaml` TTL change. The plan does NOT introduce `expires_at` as a TTL field anywhere.

**Design Q4b — 429 response shape:**
```
{
  error: { code: "egress_cap_exceeded",
           message: "Monthly egress cap exceeded; upgrade or wait for reset.",
           details: {
             monthly_egress_bytes: <current>,
             egress_quota_bytes: <2 * plan_tb_cap_bytes>,
             period: "YYYY-MM",
             resets_at: "<first-of-next-month ISO>",
             cta: { kind: "dashboard", path: "/usage" }
           }
         },
  request_id: "..."
}
```

## Files to touch
- `src/types/dynamo.ts` — add `monthly_egress_bytes`, `egress_period` to USAGE shape; new `MonthlyEgressUsage` interface for `USAGE#<YYYY-MM>` row (PK `OWNER#<id>`, SK `USAGE#<YYYY-MM>`, fields `monthly_egress_bytes: number`, `period: string`, `ttl: number`).
- `src/types/index.ts` (Account interface) — add optional `plan_tb_cap_bytes?: number` field on the ACCOUNT shape.
- `src/lib/billing-plan.ts` — **NEW**. Single helper `getPlanTbCapBytes(account: Account | undefined): number` with `DEFAULT_PLAN_TB_CAP_BYTES = 1_000_000_000_000` const fallback. Used by `tier-guard.ts` and `billing-status.ts` to guarantee identical scale.
- `src/lib/tier-guard.ts` — add `checkEgressCap(db, ownerId, deltaBytes)`. Reads `USAGE#<YYYY-MM>` row + ACCOUNT row, calls `getPlanTbCapBytes(account)`, computes `egress_quota_bytes = 2 * planTbCap`, returns discriminated-union verdict. Add `'egress_cap_exceeded'` to `TierGuardCode` union at line 34-36.
- `src/handlers/get-signed-url.ts` — at line 91-93, before `signExternalOriginal`, call `checkEgressCap(db, asset.owner_id, asset.size_bytes)` (only for owner reads — see step 4). On `ok: false` return `error(429, ...)`. At lines 122-136 (the existing `bandwidth_bytes` ADD, which PR 0 already gated on `&& isOwner`), ALSO write the monthly sibling row with `ADD monthly_egress_bytes :bytes SET ttl = :ttl, period = :period, updated_at = :now`.
- `src/handlers/billing-status.ts` — at lines 18-22 add a parallel `db.get(ownerPk(ownerId), 'USAGE#<YYYY-MM>')`. In the response object (line 31-53) add four new fields: `egress_used_bytes` (from monthly row, default 0), `egress_quota_bytes` (= `2 * getPlanTbCapBytes(account)` — IDENTICAL math to tier-guard), `plan_tb_cap_bytes` (= `getPlanTbCapBytes(account)`), `next_reset_at` (= first-of-next-month ISO). Also expose `plan_cadence` if available on the account row (or leave it `null` for now — dashboard tolerates).
- `src/handlers/get-usage.ts` — extend response (lines 23-49) to include `monthly_egress_bytes` from the sibling row.
- `src/handlers/cognito-post-confirm.ts:55-58` — bootstrap the monthly sibling row at signup; ALSO seed `plan_tb_cap_bytes = DEFAULT_PLAN_TB_CAP_BYTES` on the new ACCOUNT row.
- `src/handlers/bootstrap-key.ts:107-109` — same monthly-row bootstrap.
- `dashboard/src/routes/usage/+page.svelte:60-72` — drop the `TODO(#381)` comments around the egress block and storage cap; rename "NEXT BILL" → "NEXT RESET" (lines 99-108) and bind to `next_reset_at` instead of `next_bill_at` (rename in the `{@const}`); delete the "BUY EGRESS PACK" CTA at lines 149-157; rewire "UPGRADE PLAN" at lines 139-148 to a link to `/account` with a tooltip "Self-serve plan changes coming soon".
- `dashboard/src/lib/api/types.ts` — extend `BillingStatus` with `egress_used_bytes`, `egress_quota_bytes`, `plan_tb_cap_bytes`, `next_reset_at`.
- `template.yaml` — **NO TTL CHANGE**. The table already enables `AttributeName: ttl` at lines 375-377. Document the new row's `ttl` field in inline comment near the SWEEPLOG comment if one exists (search for "TTL" in template.yaml).

## New files
- `src/lib/billing-plan.ts` — `getPlanTbCapBytes` + `DEFAULT_PLAN_TB_CAP_BYTES` constant.

## Step-by-step
1. **Add `plan_tb_cap_bytes` to ACCOUNT** — extend the Account type in `src/types/index.ts`. Bootstrap the field with `DEFAULT_PLAN_TB_CAP_BYTES` in `cognito-post-confirm.ts:55-58`. Write a one-shot migration helper (in `scripts/backfill-plan-tb-cap.ts` if a `scripts/` dir exists, else inline as a runbook step) that scans existing ACCOUNT rows and sets `plan_tb_cap_bytes` where missing — idempotent, dry-run-able. *Verifiable: tsc clean; cognito-post-confirm test extended to assert the field is written; backfill script smoke-tested with a dry-run flag.*
2. **Define the sibling-row shape** in `src/types/dynamo.ts` — `MonthlyEgressUsage` interface uses `ttl: number` (Unix seconds, NOT `expires_at`). *Verifiable: tsc clean.*
3. **Add `getPlanTbCapBytes` helper** in `src/lib/billing-plan.ts`. *Verifiable: 3 unit tests — populated field, missing field falls back to default, zero/negative coerced to default.*
4. **Add `checkEgressCap` helper** in `src/lib/tier-guard.ts` — reads `USAGE#<YYYY-MM>` (use `monthFromDate(new Date())` helper, formatted `YYYY-MM`), reads ACCOUNT row, calls `getPlanTbCapBytes`, computes projected total, returns discriminated-union verdict. *Verifiable: 6 new unit tests in `tests/unit/lib/tier-guard.test.ts`: (a) ok under cap, (b) ok no usage row, (c) 429 when projected > 2×cap, (d) `+ deltaBytes` projection actually projects, (e) no ACCOUNT row → default cap, (f) period boundary read.*
5. **Wire `checkEgressCap` into `get-signed-url.ts`** before line 91's `signExternalOriginal` call. ONLY enforce for owner reads (`isOwner === true`) — cross-owner egress is the asset owner's problem, but on a 429 you'd be blocking the wrong person. Mirrors the bandwidth-counter ownership rule fixed in PR 0. On `ok: false` return `error(429, verdict.code, verdict.message, requestId, verdict.details)`. *Verifiable: new tests in `tests/unit/handlers/get-signed-url.test.ts` cover (a) owner 429 path, (b) shape of details, (c) cross-owner does NOT trigger the cap check, (d) success path still issues the URL.*
6. **Bump the monthly row** at `get-signed-url.ts:122` (alongside the existing `bandwidth_bytes` ADD) — issue `ADD monthly_egress_bytes :bytes SET ttl = :ttl, period = :period, updated_at = :now` on `USAGE#<YYYY-MM>` ONLY when `isOwner` (same gate as bandwidth — set by PR 0). Compute `ttl = firstOfMonth(date+2 months).getTime()/1000` (Unix seconds; matches `template.yaml:375-377` table TTL attribute name `ttl`). *Verifiable: extend get-signed-url tests for monthly row update; assert `ttl` field name (NOT `expires_at`).*
7. **Surface the counter** in `billing-status.ts` and `get-usage.ts` — read `USAGE#<YYYY-MM>` in parallel; populate four fields. `billing-status.ts` computes `egress_quota_bytes = 2 * getPlanTbCapBytes(account)` — IDENTICAL to `tier-guard.ts` (same helper). *Verifiable: extend `tests/unit/handlers/billing-status.test.ts` asserting the four fields, AND that `egress_quota_bytes` matches what `checkEgressCap` would compute for the same account row.*
8. **Bootstrap the monthly row at signup** in `cognito-post-confirm.ts` and `bootstrap-key.ts`. *Verifiable: existing tests extended to assert two USAGE rows + `plan_tb_cap_bytes` on ACCOUNT.*
9. **Dashboard wiring** — update `dashboard/src/lib/api/types.ts`; drop the `TODO(#381)` comments; rename "NEXT BILL" → "NEXT RESET"; delete the BUY EGRESS PACK CTA; rewire UPGRADE PLAN to `/account`. *Verifiable: `npm run build` + `svelte-check` clean.*
10. **Operator backfill** — run the `plan_tb_cap_bytes` backfill from step 1 in prod before the F deployment. The monthly egress counter starts at 0 next month for every owner; no egress backfill needed (cumulative `bandwidth_bytes` is unaffected). Document this in PR body and the runbook.

## Tests
- `tests/unit/lib/billing-plan.test.ts` — 3 new cases per step 3.
- `tests/unit/lib/tier-guard.test.ts` — 6 new cases per step 4.
- `tests/unit/handlers/get-signed-url.test.ts` — owner 429, cross-owner pass-through, monthly-row `ttl` field name, `bandwidth_bytes` not bumped on 429.
- `tests/unit/handlers/billing-status.test.ts` — four new fields populated; assert `egress_quota_bytes === 2 * planTbCap` shared math.
- `tests/unit/handlers/get-usage.test.ts` — `monthly_egress_bytes` plumbed.
- `tests/unit/handlers/cognito-post-confirm.test.ts` and `bootstrap-key.test.ts` — dual USAGE-row creation + `plan_tb_cap_bytes` on ACCOUNT.
- *(no new integration test for TTL behaviour — DDB TTL has up to 48h delete latency, untestable in unit; covered by manual ops verification in the runbook.)*

## Validation
- `npm ci --prefer-offline`
- `npx tsc --noEmit`
- `npx vitest run`
- `sam build`
- `cd dashboard && npm ci --prefer-offline && npx svelte-check --tsconfig ./tsconfig.json && npm run build`

## Risks
- **TTL attribute conflict** — resolved by Q7: use `ttl` (matches template.yaml:375-377). NO template.yaml change.
- **Plan-TB cap missing on ACCOUNT row** — resolved by step 1: backfill script + bootstrap default. `getPlanTbCapBytes` defaults to `DEFAULT_PLAN_TB_CAP_BYTES` if absent so pre-backfill rows are not zero-capped.
- **Egress quota scale mismatch** — resolved by Q4: single helper `getPlanTbCapBytes` consumed by both `checkEgressCap` (tier-guard) and `billing-status.ts`. Tests assert the two compute identically.
- **Cumulative `bandwidth_bytes` repurposing** — explicitly NOT done. Sibling row keeps it isolated. `daily-snapshot.ts:38-66` and `cost-calculator.ts:38-53` keep their cumulative semantics.
- **DDB TTL up to 48h delete latency** — harmless: signing path computes period from `new Date()` so always reads/writes the correct month row. Documented.
- **Period boundary race** — accept: an URL issued at 23:59:59 reading `USAGE#<MM>` and the bump completing on `USAGE#<MM+1>` is conservative for hard-cap UX. Pinned by step 4 test (f).

---

## Sequencing
1. **PR 0** (hotfix) — lands first. Restores `kstore_managed` routing + cross-owner bandwidth gate. Open against main.
2. **PR I** (B2→R2 swap) — depends on PR 0. Mechanical rename + legacy-B2 read slot.
3. **PR F** (metering + hard-cap) — depends on PR I. New monthly row + 429 + dashboard wiring.
4. Update epic spec in a follow-up docs PR (NOT in 0/I/F): rename "B2" managed-default prose to "R2", rewrite F section to describe the metering-only scope, defer Stripe integration to a future epic, document the legacy-B2 read-slot mitigation.

## Open question requiring user confirmation BEFORE PR 0
**Has #393 ever run in a deployed stage?** (Look for `MANAGED_DEFAULT_ENABLED === 'true'` in any deployed stage's environment + any populated MIGRATION rows in the prod table.) If yes, prod owners' migrated downloads have been failing — PR 0 deployment unblocks them. If no, PR 0 is purely forward-fix. Either way PR 0 ships.
