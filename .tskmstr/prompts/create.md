# Create a ticket

Draft a new ticket for this repository. Keep the ticket body human-readable;
put the detailed plan in thatch memory, not the ticket.

## Recall thatch memory first

Before drafting, search thatch memory for context relevant to this repo and the
work you are about to describe — prior plans, conventions, and decisions — so
the ticket agrees with what the project already knows.

## Draft with the thatch skill

Use the `thatch-ticket-description` skill to draft the ticket: clear sections,
bold/italic emphasis for scanning, and no invented requirements.

## Write the plan back to thatch memory

Record the detailed plan and background you gathered in thatch memory so a later
session can recall it. Keep the ticket body itself concise and readable.

## Repo-specific context

- **Backend:** tickets are GitHub issues on `jowi-dev/grpy`. Create them
  with `gh issue create -R jowi-dev/grpy` or `tm ticket create`.
- **Project:** grpy is a Rust (edition 2024) terminal app built with a Nix
  flake. It finds concerts at nearby venues, lets the user pick shows in a
  TUI, and creates Google Calendar events with ticket/show links.
- **Data strategy (decided):** venue-website scraping is the primary source,
  iCal/JSON-LD first with an LLM extraction fallback. OpenStreetMap Overpass
  finds venues. Ticketmaster is optional and secondary. Duplicate calendar
  events are never acceptable.
- **Body shape:** follow the existing issues.
  1. One or two sentences on the purpose of the change.
  2. An optional bold section such as **Sketch**, **Tiers**, or
     **Config contents**, or a short Rust code block, when the design needs it.
  3. An **Acceptance criteria** checklist (`- [ ] ...`), with a test
     criterion for each behavior.
  4. A final `Depends on: #N, #M` line when other issues must land first.
- **Labels:** apply one or more area labels: `core`, `data-source`,
  `scraping`, `calendar`, `tui`, `personalization`, `infra`. Add `spike` for
  research or decision tickets. Leave the `tm:status/*` labels alone; tm
  manages them.
- **Tracking issue:** #16 is the MVP dependency graph, grouped into lanes A
  to F. When a new issue belongs to the MVP, say which lane it fits and what
  it depends on, so #16 can be updated.
- **Testing constraint worth stating in criteria:** tests cannot use the
  network, because `nix flake check` runs them in the Nix sandbox. Scraping
  and provider work needs fixture files or the in-memory fake provider.
- **Quality gates a ticket's work must pass:** `cargo fmt --all -- --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`
  (all inside `nix develop`), then `nix flake check`. CI for these is tracked
  in #14 and does not exist yet.
