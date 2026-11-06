# Workspace Pull, Fetch, and Log Filter Keys Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Review and run Pull or Fetch across the current Roots scope and move the Log dropdowns onto the Space prefix.

**Architecture:** Shell owns the update dialog, including its reviewed targets, worker, and per-repository results. Log extends its existing key sequence picker. The command catalogue, help palette, and configuration defaults describe the same keyboard paths.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, and local bare remotes for tests.

---

### Task 1: Route normal-mode keys

- Change `config/default.toml`, `src/config/keys.rs`, and the handlers and help for Log, Push, and Merge.
- Use `p/f` for workspace updates. Move Push review and its local fetch to `P/F`; use `F` in Merge to return to its files. Keep Log search on `/` and use `Space b/u/d/p` for dropdowns. Move the branch tree's own search and selection to `S` and `s` (selection also keeps `Enter`).
- Extend `src/ui/key_sequence.rs` and `src/ui/help.rs` so suffix hints and executable help use the same routing as direct input. Cover pending prefixes, remaps, and editor isolation in the existing Shell and Log tests.

### Task 2: Review and execute updates

- Add `src/shell/update.rs` and connect it to `src/shell.rs`. Show the scoped repositories, skipped targets, Merge/Rebase radio choices for Pull, and a Cancel-focused footer. Run Git only on confirmation, one repository at a time in a worker.
- Validate the worktree and reviewed local branch, HEAD, and upstream or fetch remotes before starting each operation. Disable recursive submodule operations; let Git handle integration and keep conflicts available in Merge. Refresh Log after Fetch and the workspace after Pull. Keep per-repository results accessible in the dialog.
- Use temporary repositories and bare remotes in `src/shell/update_tests.rs`. Check cancel, scope, fetch without integration, both pull strategies on diverged branches, stale plans, and mixed success.

### Task 3: Document and verify

- Update `README.md`, `docs/prd.md`, `docs/design.md`, `docs/architecture.md`, and `docs/configuration.md` for scope, keys, and the update workflow.
- Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`. Use the Ratatui test backend to check compact review layout and mouse controls.
