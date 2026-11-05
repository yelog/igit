# Commit and Push Write Boundary Fix Plan

Goal: Paths and branches described on the confirmation page must match the actual operation. Push must neither rewrite remote history nor publish submodule versions unavailable from a newly declared source.

Reuse the existing private index, commit plans, remote tip checks, and leases. Reproduce each case against the old implementation in temporary Git repositories, then verify the result against the worktree, index, committed tree, or bare remote references.

1. **Commit and AI file scope** (`src/commit/backend.rs`): replace a tracked file with a directory of the same name, checking only deletion of the original file. Verify that untracked children enter neither the commit nor the AI patch. The reverse replacement must not delete unchecked paths either. Compare actual changed paths with the checked scope.
2. **Target branch on the confirmation page** (`src/commit.rs`, `src/shell.rs`): from another terminal, switch to a new branch at the same SHA, then refresh, review, and commit. The confirmation page must show the branch bound to the plan. Switching again after review must reject execution.
3. **Submodule sources** (`src/push/submodules.rs`): keep the gitlink SHA but change the `.gitmodules` URL from a remote containing that commit to an empty remote. The parent remote must remain unchanged even when the change appears only in an intermediate commit. Push may succeed when the new source actually contains the commit.
4. **Push fast-forward relationship** (`src/push.rs`): use local replacement references to make unrelated commits appear to be a fast-forward. Validate against the unreplaced object graph, confirming that push is rejected and the bare remote branch remains unchanged. Preserve concurrent-update protection after the check.

Integration validation: `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`. All remotes reside in temporary directories; AI regressions do not call real models.
