+++
schema_version = 1
id = "01M3WG8XK8YJR3Z2BG1NFDD442"
title = "Check on macOS runners and let Claude fix dependency updates"
date = "2026-10-01"
status = "proposed"
tags = ["ci", "dependencies"]
supersedes = []
superseded_by = []
depends_on = []
related_to = []
+++
## Decision

Focal is checked on GitHub Actions' macOS runners: `.github/workflows/check.yml` runs `mise run check` (rustfmt, Clippy with warnings as errors, all tests) on every push to `main` and every pull request.

Dependencies are kept current by Dependabot (`.github/dependabot.yml`): Rust crates weekly, GPUI Kit in a pull request of its own and everything else grouped in one, GitHub Actions monthly. When Dependabot's Rust update fails the check, `.github/workflows/fix-dependency-update.yml` runs Claude Code (`anthropics/claude-code-action`) on the same branch to adapt Focal's code, commits what it changed, and starts the check again. A person reviews and merges every update.

Niklas asked on 2026-10-01 for Dependabot or similar, with an agentic session fixing the bump pull requests; the setup around it is proposed.

## Context

- GPUI changes its API often; GPUI Kit pins a GPUI revision and Focal pins GPUI Kit exactly (`=0.7.0`), so a GPUI update arrives as one GPUI Kit pull request, the one most likely to need code changes.
- Focal builds only on macOS (GPUI's Metal renderer, AppKit, WebKit), and its tests use macOS services (the spell checker, Core Text fonts). Linux containers cannot build or test it, so the check runs natively on macOS instead of in a Dagger pipeline (Niklas's default for CI, which keeps native platform checks where containers cannot cover them).
- Commits pushed with the workflow's token do not start other workflows, except `workflow_dispatch`; the fix workflow therefore dispatches the check itself. The dispatched check runs as `github-actions`, not Dependabot, so a fix that still fails is not retried in a loop.

Alternatives considered:

- **Renovate**: more configurable (grouping by upstream repository, schedules), but another app to install; Dependabot covers what is needed.
- **Dependabot without the fix workflow**: every GPUI Kit update would wait for a manual session.
- **Letting Claude push and open its own pull requests**: the workflow commits instead, so Claude's tools stay limited to Cargo, mise and reading and editing files.

## Consequences

- The repository needs the secret `CLAUDE_CODE_OAUTH_TOKEN` (from `claude setup-token`) for the fix workflow; without it, failing updates simply wait for a person.
- The fix job compiles and runs the new dependency's build scripts while it holds that token and a token that can push. It only takes Dependabot's branches in this repository, but a compromised crate release could read the secrets; rotating the token is the remedy.
- Once the fix workflow pushes to a Dependabot branch, Dependabot stops rebasing that pull request; recreate it with `@dependabot recreate` if needed.
- `svgbob` is held back on purpose and ignored by Dependabot.
- macOS runner minutes are free for this public repository.
