# Push Rejection Diagnostics Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Push failures identify the remote policy reason instead of presenting only the hook name.

**Architecture:** Keep captured Git error output intact in per-repository Details and notification history, but derive a bounded one-line summary from the execution phase and explicit server errors. Cover merged commits whose generated subject fails a remote issue-key policy without inventing an issue key or bypassing remote hooks.

**Tech Stack:** Rust, system Git, existing Push worker and notification tests.

---

### Task 1: Summarize remote verification failures

**Files:**
- Modify: `src/push.rs`
- Test: `src/push.rs`

**Step 1:** Add a failing notification test containing a hook progress banner followed by an issue-key policy error.

**Step 2:** Run `python3 scripts/cargo_serial.py test --locked --lib push::tests::  --quiet` and confirm the test fails.

**Step 3:** Replace the first-line-only notification summary with a compact message that retains the execution phase and prioritizes explicit server errors over progress banners. Keep full error output in notification history.

**Step 4:** Re-run the focused test.

### Task 2: Preserve failure and success outcomes after an update

**Files:**
- Modify: `src/push/update/tests.rs`
- No runtime changes expected unless the regression fails.

**Step 1:** Reproduce a diverged update followed by remote rejection and verify the exact generated merge commit is offered to the remote.

**Step 2:** Verify the error identifies both the successful update and the failed publication.

**Step 3:** Verify retry preserves the merged HEAD, performs no additional merge, and still respects remote hooks even when local pre-push hooks are disabled. Keep existing successful-update coverage.

### Task 3: Verify end to end

**Files:**
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/prd.md`

**Step 1:** Document that remote policy rejections keep captured error output in Details and notification history while the summary identifies the policy reason.

**Step 2:** Run focused Push tests, formatting, Clippy, and documentation link checks.

**Step 3:** Run the full regression if the runtime change affects shared notification behavior.

**Step 4:** Do not modify or push `/Users/yelog/workspace/lenovo/moss/moss-service-mes` without the correct issue key.

## Verification

- The original first-line-only notification failed the new policy-message test.
- All 90 Push tests passed after the fix, including a native pre-receive rejection
  after a successful merge and a retry that preserves the same local HEAD.
- All four Shell update-strategy tests passed.
- Clippy with warnings denied and the development binary build passed.
- The business repository was inspected read-only. Its working tree is clean;
  the remote remains at the fetched commit and the two local commits remain
  unpublished. Repair requires the actual issue key and authorization to rewrite
  those messages and publish.
