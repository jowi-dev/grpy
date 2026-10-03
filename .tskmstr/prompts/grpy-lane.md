# grpy work lane

Autonomous work session for a single ticket in this repository. Do
not scope-creep beyond the named ticket.

grpy is a Rust (edition 2024) terminal app that finds concerts at nearby
venues and adds them to Google Calendar. Tickets are GitHub issues on
`jowi-dev/grpy`; issue #16 holds the MVP dependency graph.

## Start

1. Run `tm ready <KEY>` and stop if it reports the ticket blocked.
2. Work only `<KEY>`. Note unrelated bugs or cleanup as follow-ups
instead of fixing them here.
3. Read the issue (`gh issue view <KEY> -R jowi-dev/grpy`). Its
**Acceptance criteria** checklist is the definition of done, and its
"Depends on" line names work that must already be merged.
4. Run every cargo command inside the Nix dev shell: `nix develop -c <cmd>`
(or an already-entered `nix develop` / direnv shell). The toolchain,
rustfmt, clippy, and cargo-nextest come from `flake.nix` and
`rust-toolchain.toml`, not the host.

## Workflow

- Write a failing test before the implementation that makes it pass.
Run it and confirm it fails for the expected reason first.
- Keep commits small and focused, one logical change per commit, in
imperative mood ("Add venue ID parsing", not "Added ...").
- Do not add a `Co-Authored-By: Claude` trailer to commits.
- Branch off `main`; never commit directly to `main`. Open the PR with
`tm pr create` so the ticket is associated.
- Update doc comments and `README.md` when a public API, behavior, or
developer command changes.

## Hazards

- **Tests must not touch the network.** `nix flake check` builds the
package in the Nix sandbox, which runs `cargo test` with no network
access. Scraper and provider tests use fixture HTML/JSON/ICS files and
the in-memory fake provider, never live sites or APIs.
- **The Nix build only sees git-tracked files.** `git add` new source,
fixture, and module files before running `nix flake check`, or the
build will fail with missing files.
- **Keep `Cargo.lock` in sync.** Add dependencies with `cargo add` and
commit the resulting `Cargo.lock`; the flake builds from it. Never
hand-edit it.
- **Do not change `flake.lock` or `rust-toolchain.toml`** unless the
ticket is about the toolchain or build.
- **Never commit secrets.** `.env` is gitignored and holds local keys.
Google OAuth client secrets, refresh tokens, and provider API keys stay
out of the repo, out of test fixtures, and out of logs.
- Do not run `grpy` against a real Google Calendar from a lane; it
creates real events on the operator's account.

## Before finishing

Leave all of these green, in this order, from the repo root:

```sh
nix develop -c cargo fmt --all -- --check
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo test
nix flake check
```

If `cargo fmt --check` fails, run `nix develop -c cargo fmt --all`, then
commit the formatting. `nix flake check` builds the package and runs its
tests in the sandbox, so run it last, after staging new files.
