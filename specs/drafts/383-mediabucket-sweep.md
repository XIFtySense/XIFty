<!-- loswf:plan -->
# Plan #383 (sub-PR H of epic #373): Sweep AWS S3 source bytes after 30-day grace

## Problem
After #393 (sub-PR D, merged) bulk-migrated platform-stored asset bytes from `MediaBucket` (AWS S3) to managed B2 in-place, the worker hard-deletes each canonical source object as part of the COPY → HEAD → CHECKSUM → SWAP → DELETE ordering. What it does **not** delete: thumbnails / previews stamped sibling-of-original by `process-image.ts`, abandoned multipart upload parts, EXIF/preview artefacts, and any source object whose worker delivery succeeded post-swap but pre-delete (rare: hard-delete is best-effort + log per the worker docstring at `src/handlers/migration-worker.ts:22-25` of #393). Sub-PR H is the per-owner, scheduled, post-grace cleanup that reclaims those orphaned MediaBucket bytes for owners whose migration is verified complete and stable. Issue acceptance criteria narrow this to a 30-day grace gate keyed on the per-owner MIGRATION batch row, with a kill-switch and metrics. Per the user prompt, dry-run-first, audit log, and CloudWatch reclaimed-bytes/objects metrics are added on top as additive safety / observability — they do not change the issue's acceptance criteria.

## Approach
Mirror two patterns already in main. (1) `DailySnapshotFunction` in `template.yaml` (origin/main lines 2484-2510) for the scheduled-rule shape — a `Type: Schedule` event source, Lambda timeout 300, daily cron. (2) `AccountDeleter.deleteOwner` in `src/core/account-deleter.ts:99-120` for the per-owner ListObjectsV2 + batched DeleteObjects + continuation-token loop. The handler enumerates eligible owners by querying the existing MIGRATION batch rows from `migration-status-repo.ts` (#393), filters to `status === 'completed' AND completed_at < now - 30d AND failed_assets === 0`, then for each owner does a paged S3 sweep under `Prefix: ${ownerId}/`. Each candidate object is cross-checked against the active asset rows for that owner (loaded once per owner via `AssetRepository.list` cursor-loop into an in-memory `Set<storage_key>` of all referenced keys: canonical originals, thumbnails-from-`asset.thumbnails.thumb/preview`, and the swapped `original_reference.key` for managed assets which lives in B2 not MediaBucket — those keys won't appear in MediaBucket ListObjectsV2 output anyway). Anything in MediaBucket under the owner prefix that is NOT in the referenced-set is an orphan; it is logged + counted in dry-run mode and `DeleteObjects`-batched in apply mode. The per-asset-row lookup approach is rejected (cost + latency), the GSI-on-storage_key approach is rejected (no such GSI exists, would require schema migration), and the DDB Scan approach is rejected (existing `daily-snapshot` already pays one Scan per night, this would double it; the cursor-loop on `gsi2` per eligible owner is bounded by completed-batch count which scales with owners, not assets). Multipart leftovers (`AbortMultipartUpload`) are explicitly out of scope per the issue's narrow file list — tracked as a follow-up note in the runbook section. Apply mode is dry-run-required-first via a per-owner DDB sentinel (`SWEEP#<owner_id>` row stamps `last_dry_run_at`); apply refuses to operate on owners without a dry-run inside the last 7 days. Two-phase commit. CloudWatch metrics emitted via `console.log` EMF (no extra IAM): `BytesReclaimed`, `ObjectsDeleted`, `OwnersProcessed`, `OwnersSkipped`. Single Lambda (not Step Function) — at 300s timeout the per-invocation budget covers ~30 owners at typical (sub-1k thumb-objects/owner) volume; multi-TB owners get an explicit page-cursor handoff via the SWEEP sentinel row's `continuation_token` field that the next nightly invocation resumes from. Step Function orchestration is rejected as overkill given (a) the kill-switch needs a single env var read on entry (no SFN gain), (b) #393's worker already proved single-Lambda + DDB sentinel is enough for resumable bulk work, and (c) the SAM diff stays small.

## v4 design decisions (concentrated)

### DECISION 1 — `markCompleted` must stamp `completed_at`
`src/lib/migration-status-repo.ts` `markCompleted` (origin/main lines 230-247) currently writes `SET #s = :done, updated_at = :now`. The 30-day grace check needs a stable `completed_at` field that doesn't drift on subsequent counter writes. Plan amends `markCompleted` to additionally `SET completed_at = if_not_exists(completed_at, :now)`. The `MigrationBatchRow` interface (lines 38-50) gains `completed_at?: string`. Pure additive — no callsite changes.

### DECISION 2 — Owner enumeration via DDB Scan filtered by sk-prefix + status
No GSI exists for `sk begins_with MIGRATION# AND status = completed`. The sweep adds one Scan per nightly run, mirroring `daily-snapshot.ts:26-32`. Filter: `FilterExpression: 'begins_with(sk, :pfx) AND #s = :done AND failed_assets = :zero'`, `ExpressionAttributeNames: { '#s': 'status' }`, `ExpressionAttributeValues: { ':pfx': 'MIGRATION#', ':done': 'completed', ':zero': 0 }`. Filters out per-asset sub-rows (`MIGRATION#<batch>#ASSET#<id>`) by additionally rejecting any sk that contains `#ASSET#` in the JS post-filter step. The Scan paginates with `ExclusiveStartKey` until exhausted.

### DECISION 3 — Per-owner referenced-key set via cursor-loop
`AssetRepository.list` is hard-capped at 100/page (the lesson from #393). The sweep loops over `repo.list(ownerId, { status: 'active', limit: 100, cursor })` until `!page.cursor`, collecting into `Set<string>`:
- `asset.storage_key` (covers canonical platform originals and post-swap rows)
- `asset.thumbnails?.thumb` and `asset.thumbnails?.preview` (sibling thumbs in MediaBucket regardless of original location — see `src/handlers/process-image.ts:146` "thumbnails always live in MediaBucket")
- For now-managed assets, `asset.storage_key` may already point at the managed-B2 key; that's harmless since MediaBucket ListObjectsV2 output won't contain it. The set's job is to *retain* anything still referenced; over-inclusion is safe.

Status filter `'active'` is intentional — `deleted` / `archived` rows do not protect their bytes from sweep (those bytes were already released by the lifecycle that produced those statuses). Document this in the handler doc-comment with a note pointing to `delete-asset.ts` and a follow-up TODO if operators want to widen the protection-set.

### DECISION 4 — Cross-reference inside the S3 page loop (memory-bounded)
For owners with millions of objects, holding the full S3 page list and the full referenced-set is bounded: ListObjectsV2 returns ≤1000 objects/page, the referenced-set is bounded by `count(active assets) × 3` (storage_key + thumb + preview) per owner. At 100k assets that's a 300k-string set ≈ 30 MB — within the 1024 MB Lambda. For >100k-asset owners, a `console.warn` advises the operator to run sweep with a forced single-owner override (env-var `SOURCE_SWEEP_OWNER_ID` — a manual diagnostic mode) and tracks the gap as an out-of-scope follow-up.

### DECISION 5 — Two-phase apply via SWEEP sentinel row
DDB row layout:
```
pk = OWNER#<owner_id>
sk = SWEEP#SOURCE
```
Fields: `last_dry_run_at`, `last_dry_run_candidates`, `last_dry_run_bytes`, `last_apply_at`, `last_apply_objects_deleted`, `last_apply_bytes_reclaimed`, `last_continuation_token`, `mode_history` (last 5).

Apply mode rules:
- **dry-run mode is the default** (env `SOURCE_SWEEP_MODE=dry_run` or unset).
- **apply mode requires** `SOURCE_SWEEP_MODE=apply` AND a SWEEP sentinel with `last_dry_run_at >= now - 7d` for the owner. Otherwise: skip the owner with reason `apply_without_recent_dry_run`, increment the `OwnersSkipped` metric, log the skip, continue with the next owner.
- **kill-switch** `SOURCE_SWEEP_ENABLED=false` causes the handler to log + return early with zero work. Default: `true` only when explicitly set; treat unset as `false` to default-safe.

### DECISION 6 — Audit log per deleted key
`apply` mode writes a per-key audit row before the `DeleteObjects` call:
```
pk = OWNER#<owner_id>
sk = SWEEPLOG#<iso_timestamp>#<sha1(key)>
```
Fields: `key`, `size_bytes`, `last_modified` (from ListObjectsV2 `LastModified`), `apply_run_id` (ulid). Written via `BatchWriteCommand` in the same loop as the S3 batch-delete. If the BatchWrite throws, the S3 delete is **not** issued for that batch — fail-stop. Failure does not corrupt state because S3 delete hasn't run; the next nightly retry sees the same orphans.

### DECISION 7 — IAM scope-down
- `S3CrudPolicy: BucketName: !Ref MediaBucket` (matches `AccountDeleter`).
- `DynamoDBCrudPolicy: TableName: !Ref KstoreTable` (read MIGRATION rows, write SWEEP/SWEEPLOG rows).
- No `states:*`, no SQS, no KMS — sweep doesn't touch encrypted refs.

### DECISION 8 — CloudWatch metrics via EMF
Single `console.log` JSON line per owner with `_aws.CloudWatchMetrics` schema; no extra IAM. Namespace `Kstore/MigrationSweep`. Dimensions: `Stage`, `Mode`. Metrics: `BytesReclaimed`, `ObjectsDeleted`, `OwnersProcessed`, `OwnersSkipped`, `CandidatesFound`. Mirrors the EMF style already used in `src/lib/stripe-usage.ts` (verify on builder).

## Files to touch
- `template.yaml` (origin/main has BulkMigration block at lines 1221-1410; DailySnapshot at 2484-2510) — append a new `# ── SourceSweep (issue #383) ─────` block after the BulkMigration block. Add `S3SourceSweepFunction` mirroring `DailySnapshotFunction` (Schedule event), with `Timeout: 300`, `MemorySize: 1024` (need headroom for the in-memory referenced-set), env vars `MEDIA_BUCKET`, `SOURCE_SWEEP_ENABLED`, `SOURCE_SWEEP_MODE`, optional `SOURCE_SWEEP_OWNER_ID`, optional `SOURCE_SWEEP_GRACE_DAYS` (default 30), optional `SOURCE_SWEEP_DRY_RUN_FRESHNESS_DAYS` (default 7). Schedule `cron(0 5 * * ? *)` (5 AM UTC, after `daily-snapshot` at 3 AM UTC so the snapshot's accounting reflects post-sweep state — minor but tidy). Policies: `DynamoDBCrudPolicy` + `S3CrudPolicy` per DECISION 7.

- `src/lib/migration-status-repo.ts` (origin/main lines 38-50, 230-247) — DECISION 1: add `completed_at?: string` to `MigrationBatchRow`; amend `markCompleted` `UpdateExpression` to `'SET #s = :done, updated_at = :now, completed_at = if_not_exists(completed_at, :now)'`. No interface signature change. Existing tests `tests/unit/lib/migration-status-repo.test.ts` get one new assertion case.

## New files
- `src/handlers/s3-source-sweep.ts` — scheduled handler. Outline:
  ```ts
  // 1. Read kill-switch + mode env. If !enabled, log + return zero stats.
  // 2. Scan MIGRATION batch rows (DECISION 2). For each row:
  //    a. Skip if !isEligible(row, now, graceDays, failedAssetsZero).
  //    b. Skip if mode === 'apply' && !hasFreshDryRun(sentinel, freshnessDays).
  //    c. Build referencedSet via repo.list cursor-loop (DECISION 3).
  //    d. List MediaBucket under `${ownerId}/` page by page (DECISION 4):
  //         - For each object not in referencedSet:
  //             • Add to candidates (size, key, lastModified).
  //         - Resume from sentinel.last_continuation_token if set.
  //    e. Mode === 'dry_run': update SWEEP sentinel candidates + bytes,
  //       emit metrics, log first 20 candidate keys for operator review.
  //    f. Mode === 'apply':
  //         • Chunk candidates into 1000 (S3 DeleteObjects max).
  //         • For each chunk: BatchWrite SWEEPLOG rows → DeleteObjects.
  //         • Update SWEEP sentinel last_apply_*; clear continuation_token.
  //    g. Always: emit per-owner EMF metrics (DECISION 8).
  // 3. If timeout-budget low, persist continuation_token to sentinel and
  //    return — next nightly resumes (DECISION 4 paragraph 2).
  ```
  Imports: `S3Client`, `ListObjectsV2Command`, `DeleteObjectsCommand`; `DynamoDBClient`, `DynamoDBDocumentClient`, `ScanCommand`, `BatchWriteCommand`; `createDynamoClient`; `AssetRepository`; `createDefaultDocClient` + `createMigrationStatusRepo` from `migration-status-repo`. ulid for `apply_run_id`.

- `src/lib/migration/grace.ts` — pure helpers (issue's stated file path):
  ```ts
  export interface GraceCheckInput {
    completedAt?: string;       // batch row completed_at (DECISION 1)
    now: Date;
    graceDays: number;          // default 30
    failedAssets: number;       // batch row failed_assets
    status: MigrationBatchStatus;
  }
  export function isEligibleForSweep(i: GraceCheckInput): boolean;
  export interface DryRunFreshnessInput {
    lastDryRunAt?: string;
    now: Date;
    freshnessDays: number;      // default 7
  }
  export function hasFreshDryRun(i: DryRunFreshnessInput): boolean;
  ```
  No DDB / no SDK; just date arithmetic. Easy to unit-test.

- `src/lib/sweep-sentinel-repo.ts` — wrapper over the SWEEP/SWEEPLOG rows. Mirrors `migration-status-repo.ts` shape (DocClientLike interface + `createDefault` factory). Methods: `getSentinel(ownerId)`, `recordDryRun({ownerId, candidates, bytes, continuationToken?})`, `recordApply({ownerId, applyRunId, objectsDeleted, bytesReclaimed, continuationToken?})`, `writeAuditBatch({ownerId, applyRunId, items: Array<{key, size, lastModified}>})` (BatchWrite SWEEPLOG rows — chunked at 25 per BatchWrite per DDB cap, internally loops).

- `tests/unit/handlers/s3-source-sweep.test.ts` — vitest mocks. Cases (mapped to issue acceptance criteria):
  1. **In-grace skip** — completed_at = now-15d, sweep skips, no S3 ops.
  2. **Failed-asset skip** — failed_assets > 0, sweep skips.
  3. **Happy dry-run** — eligible owner, 5 objects in S3, 3 referenced, 2 candidates → sentinel updated, no DeleteObjects call.
  4. **Happy apply (with fresh dry-run)** — same fixtures, mode=apply, dry-run sentinel = now-2d → DeleteObjects called once, SWEEPLOG rows written, sentinel `last_apply_*` updated.
  5. **Apply-without-recent-dry-run skip** — mode=apply, no sentinel → owner skipped with `apply_without_recent_dry_run`.
  6. **Kill-switch off** — `SOURCE_SWEEP_ENABLED=false` → no Scan, no S3, no DDB writes; returns early.
  7. **Idempotent re-run** — apply mode after a successful apply (no new orphans) → zero deletes, sentinel `last_apply_at` updated harmlessly, no SWEEPLOG rows written.
  8. **Continuation-token resume** — sentinel has `last_continuation_token` set → ListObjectsV2 invoked with that token on first call.
  9. **`failed_assets = 0` AND `status='completed'` AND `completed_at < now-30d`** all required (cover the AND with a parameterised test).
  10. **EMF metric assertion** — capture `console.log` calls, parse the `_aws` JSON, assert metric names present.

- `tests/unit/lib/migration/grace.test.ts` — pure unit tests for `isEligibleForSweep` and `hasFreshDryRun` boundary conditions (exactly 30d, 30d-1ms, missing fields, future dates).

- `tests/unit/lib/sweep-sentinel-repo.test.ts` — DocClient stub. Cases: getSentinel returns undefined when missing; recordDryRun writes correct shape; writeAuditBatch chunks at 25.

- `tests/unit/lib/migration-status-repo.test.ts` (extend) — add a case asserting `markCompleted` writes `completed_at = if_not_exists(...)` and a second case asserting a re-call does not overwrite `completed_at`.

## Step-by-step
1. Add `completed_at` to `MigrationBatchRow` and amend `markCompleted` UpdateExpression in `src/lib/migration-status-repo.ts` — verifiable: existing tests stay green; new test asserts `if_not_exists` form and additive field.
2. Create `src/lib/migration/grace.ts` with the two pure helpers — verifiable: `tests/unit/lib/migration/grace.test.ts` passes.
3. Create `src/lib/sweep-sentinel-repo.ts` (DocClientLike pattern from #393) — verifiable: `tests/unit/lib/sweep-sentinel-repo.test.ts` passes.
4. Create `src/handlers/s3-source-sweep.ts` per the outline — verifiable: 10-case `s3-source-sweep.test.ts` passes.
5. Wire `S3SourceSweepFunction` in `template.yaml` (mirror `DailySnapshotFunction`) — verifiable: `sam build` succeeds; resource appears in CloudFormation template.
6. Smoke-run `npx tsc --noEmit && npx vitest run` end-to-end — verifiable: zero failures.
7. Update PR body runbook section with: (a) operator workflow (dry-run first, inspect SWEEP sentinel, then apply); (b) explicit out-of-scope notes for multipart leftovers + >100k-asset-owner memory cap; (c) rollback (kill-switch flip + remove SWEEP sentinels via admin tool — out-of-scope follow-up).

## Tests
Mapped per step above. The issue's stated `tests/unit/handlers/s3-source-sweep.test.ts` gets all 10 cases (covers the four acceptance-criteria cases plus 6 additive). Two new test files added (`grace.test.ts`, `sweep-sentinel-repo.test.ts`); one existing extended (`migration-status-repo.test.ts`).

## Validation
Issue's stated validation: `npm ci --prefer-offline && npx tsc --noEmit && npx vitest run`. From `.loswf/config.yaml`: `install`, `typecheck`, `tests`, `sam-build` (because `template.yaml` is touched). The `dashboard-*` and `agent-*` gates are not touched and should remain green.

## Risks
- **Memory cap on owners with >100k active assets** — referenced-set holds ≈3 strings/asset; documented + soft-warn + diagnostic single-owner override env var. Out-of-scope: streaming intersection (would require an external sorted-set or DDB GSI on storage_key, both bigger than this PR).
- **Eventual consistency** between AssetRepository.list and S3 ListObjectsV2** — a write that happened in the last few seconds could appear in S3 before in DDB. Mitigation: the sweep targets owners whose batch is `completed_at >= 30d ago`, so any post-migration write is far older than the eventual-consistency window. Still: dry-run-first gate prevents an unnoticed delete from a freshly-written object — operators are required to inspect candidates.
- **Multipart upload leftovers (`AbortMultipartUpload`)** — out-of-scope per the issue's stated file list. Tracked as a follow-up note in the runbook. ListObjectsV2 won't surface in-flight multipart parts anyway; they need `ListMultipartUploads` + `AbortMultipartUpload` and a separate age threshold.
- **Thumbnails still living in MediaBucket for assets that were swapped to managed B2** — by design (per `process-image.ts:146` "thumbnails always live in MediaBucket regardless of where the original lives"). The referenced-set includes them; sweep will not delete them.
- **`completed_at` backfill** — pre-#383 batch rows from #393 were written without `completed_at`. The `if_not_exists` write covers new completions; for already-completed batches, `completed_at` will be missing forever unless a one-shot backfill is run. Acceptance: `isEligibleForSweep` treats missing `completed_at` as ineligible (default-safe — those owners are skipped until a backfill or until the next batch completes). Documented in the handler doc-comment; backfill is a trivial follow-up admin endpoint if operators want to sweep the early adopters.
- **Apply mode racing with a still-running batch** — guarded by `status === 'completed' AND failed_assets === 0 AND completed_at < now-30d`. A new batch starting after a prior completion would not affect the older row's eligibility, but its in-flight writes (uploads, thumbnails) would update the asset rows; the referenced-set captures those because it's loaded at sweep-time, not at batch-completion-time.
- **DDB Scan cost** — one extra Scan per night on top of `daily-snapshot`'s. Acceptable on current table size (low thousands of MIGRATION rows max). Document; revisit if MIGRATION rows ever exceed ~10k.
