# Pull and Fetch Activity Indicators Implementation Plan

**Goal:** Replace Checking, Pending, Running, and Updating progress labels in workspace Pull and Fetch with the existing animated spinner.

**Architecture:** Reuse `ui::loading::Indicator` with one clock per update dialog. Keep the repository state machine, reviewed scope, confirmation guards, Git execution, and final results unchanged.

**Tech Stack:** Rust, Ratatui, Crossterm, existing icon modes and palette.

---

1. Add regression coverage for animation-only process states in the title,
   repository status column, Details, summary, and footer.
2. Expose the shared indicator glyph and tick update dialogs through Shell.
   Use blue for checks and active work, and muted spinners for queued rows.
3. Keep repository counts and numeric progress visible; retain Ready before
   confirmation and readable success, failure, and skipped outcomes.
4. Verify loading, queue-to-running transitions, partial results, completion,
   errors, and narrow layouts in all icon modes. Run the affected update tests,
   format, Clippy, and both binary builds through `scripts/cargo_serial.py`.
5. Update the existing Pull/Fetch design, requirements, architecture, and README.

### Verification completed

- The original text-only loading view failed the new spinner assertion before
  implementation.
- `test --locked --lib shell::update`: 39 passed, including loading and execution
  animation, confirmation guards, queue-to-running transitions, partial results,
  failed readers, and actual Git operations against disposable local remotes.
- `test --locked --lib ui::loading`: 2 passed.
- Rendering checks cover 44×12, 80×24, 120×36, and 180×36 in all three icon modes.
- `fmt --all --check` and `clippy --locked --all-targets -- -D warnings` passed.
- `build --locked --bins` rebuilt both debug executables.
- A PTY smoke run with delayed Git commands observed multiple animation frames
  during both repository checks and execution for Pull and Fetch, without
  Checking/Pending/Running/Updating labels. Review readiness and results stopped
  animation, Pull cancellation remained effective, and up-to-date local
  operations preserved HEAD, staged content, and working-tree content.

All Cargo commands above used `python3 scripts/cargo_serial.py`; the smoke run
used only disposable repositories and local bare remotes.
