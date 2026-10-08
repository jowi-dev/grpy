# 0001: Concert data source

- **Status:** Accepted
- **Date:** 2026-10-02
- **Issue:** #1 (spike), informs #6, #7, #18, #19

## Context

grpy needs upcoming shows for the venues a user follows. The direction going
in was: scrape venue websites as the primary source, cheapest extraction tier
first, LLM extraction as a fallback, Ticketmaster as an optional secondary
source.

Extraction tiers, cheapest and most reliable first:

1. iCal/ICS feed
2. schema.org `Event` JSON-LD or microdata in the page
3. Known ticketing platform/widget with predictable markup or a public API
4. LLM extraction from cleaned page text

This spike checks that plan against real venues around Pompano Beach, FL.

## Method

Each venue's events page was fetched once with a plain HTTP client (browser
User-Agent, no JS) on 2026-10-02 and checked for: linked ICS feeds, common
plugin feed URLs (`?ical=1`), JSON-LD/microdata `Event`s, framework hydration
data (`__NEXT_DATA__`), and ticketing-platform links. Pages with no usable
content in the raw HTML were re-fetched with headless Chromium
(`--dump-dom`, 15 s virtual time budget) to decide whether a headless
browser is required.

Token counts are cleaned visible text (scripts, styles, `<head>`, and markup
removed) divided by 4, a rough chars-per-token ratio. They are estimates,
not tokenizer output.

## Findings

Venues the operator named are marked ★. The rest were picked to cover a mix
of sizes in Broward, south Palm Beach, and Miami.

| Venue | Events page | Works at tier | JS-rendered? | Ticketing | Notes |
|---|---|---|---|---|---|
| ★ Hard Rock Live (Hollywood) | <https://casino.hardrock.com/hollywood/entertainment> | 3 (Ticketmaster) / 4 | No | Ticketmaster | Casino-wide listing, filter to Hard Rock Live. Cloudflare challenge script present; plain fetch still got content today, but expect it to be fragile. |
| ★ Culture Room (Fort Lauderdale) | <https://www.cultureroom.net/> | 3 (Ticketmaster IDs) / 4 | No | Ticketmaster | Site-builder page, no structured data. Each show links a Ticketmaster event ID. |
| ★ Revolution Live (Fort Lauderdale) | <https://www.jointherevolution.net/concerts/> | **1** | No | Ticketmaster | Site-wide ICS at `/events/?ical=1` (WordPress Events Manager), not linked from the page; only per-event `…/ical/` links are. Feed holds 858 events, 33 upcoming: filter by `DTSTART`. |
| ★ iTHINK Financial Amphitheatre (West Palm Beach) | <https://www.ithinkfiamp.com/> | **2** (JSON-LD) | No | Ticketmaster | Live Nation venue template; 9 JSON-LD `Event`s in the raw HTML. Same data at <https://www.livenation.com/venue/KovZpZAEkvEA/ithink-financial-amphitheatre-events>. Ticketmaster venue 106578. `westpalmbeachamphitheatre.com` is a third-party resale site, not the venue's. |
| The Amp (Pompano Beach) | <https://www.pompanobeacharts.org/events> | 4 | No | AXS | City arts site covering several venues; 8 of 88 listings are the Amphitheater. Consistent CSS classes (`event-title`, `event-location`), so a bespoke selector would also work. |
| Funky Biscuit (Boca Raton) | <https://funkybiscuit.com/calendar/> | **3** (SeeTickets) | No | SeeTickets | SeeTickets white-label plugin markup (`seetickets-calendar-event-*`), about 112 listings. |
| Crazy Uncle Mike's (Boca Raton) | <https://crazyunclemikes.com/events/> | **1** (also 2) | No | WooCommerce | The Events Calendar: linked `?ical=1` feed plus 12 JSON-LD `Event`s on the page. Feed includes some past dates. |
| Boca Black Box (Boca Raton) | <https://www.bocablackbox.com/> | **2** (hydration JSON) | No | — | Next.js + TinaCMS. All 20 events are in `__NEXT_DATA__` in the first response, so no browser is needed. |
| The Parker (Fort Lauderdale) | <https://www.parkerplayhouse.com/events> | 3 (Ticketmaster) / 4 | **Yes** | Ticketmaster | Events loaded by XHR; the AJAX endpoint returns 406 to non-browser clients. |
| Broward Center (Fort Lauderdale) | <https://www.browardcenter.org/events> | 3 (Ticketmaster) / 4 | **Yes** | Ticketmaster | Same platform as The Parker. |
| The Fillmore Miami Beach | <https://www.livenation.com/venue/KovZpZAEkedA/the-fillmore-miami-beach-events> | 2 after render / 3 | **Yes** | Ticketmaster | Live Nation page; 29 JSON-LD `Event`s only appear after JS runs, unlike iTHINK's Live Nation page, which has them in the raw HTML, so Live Nation pages aren't consistent. Ticketmaster covers it directly. |
| Mizner Park Amphitheater (Boca Raton) | <https://www.myboca.us/calendar.aspx> | none for concerts | No | — | City CivicPlus calendar has per-category ICS (`/common/modules/iCalendar/iCalendar.aspx?catID=N&feed=calendar`) and microdata, but it carries city events, not the concert lineup. No first-party concert listing found. |
| Respectable Street (West Palm Beach) | <https://sub-culture.org/> | none | No | — | Venue domain redirects to a 2022 event page. The advertised ICS is empty and The Events Calendar's REST API returns 0 events. |
| Gramps (Miami) | <https://www.gramps.com/> | none | Yes (Readymag) | — | Design-tool site with no event calendar, even after rendering. Shows are listed elsewhere (social/ticketing). |

**Best tier per venue (14):**

- Tier 1: 2 (Revolution Live, Crazy Uncle Mike's)
- Tier 2: 2 (iTHINK Financial Amphitheatre, Boca Black Box)
- Tier 3: 6 (Funky Biscuit via SeeTickets; Hard Rock Live, Culture Room,
  Parker, Broward Center, and Fillmore via Ticketmaster)
- Tier 4 only: 1 (The Amp)
- None: 3

Headless browser needed: 4 (Parker, Broward Center, Fillmore, Gramps). The
first three are Ticketmaster venues.

### LLM token cost (tier-4 candidates)

Assumes Claude Haiku 4.5 at $1 / MTok input and $5 / MTok output (list
price; verify before relying on it), a ~500-token extraction prompt, and
~50 output tokens per extracted event.

| Page | Cleaned text tokens | Events | Est. cost / fetch |
|---|---|---|---|
| Culture Room | ~450 | 15 | ~$0.005 |
| Hard Rock Live | ~1,500 | ~20 | ~$0.007 |
| The Parker (rendered) | ~1,300 | ~16 | ~$0.006 |
| Broward Center (rendered) | ~1,600 | ~18 | ~$0.007 |
| The Amp (whole arts site) | ~5,000 | 8 (of 88) | ~$0.008 |
| Funky Biscuit (if tier 3 breaks) | ~5,700 | ~60 | ~$0.02 |

Cost per page is cents or less, and output tokens dominate. Refreshing every
tier-4 venue daily comes to well under $5/month. Cost isn't the constraint;
reliability and the browser dependency are.

## Decision

1. **Keep venue scraping as the primary source**, with tiers tried in order.
   Feed discovery must **probe** known plugin URLs (`?ical=1`,
   `/events/?ical=1`), not just follow `<link>`/`<a>` tags. Revolution Live's
   feed is unlinked.
2. **Treat framework hydration JSON (`__NEXT_DATA__`) as part of tier 2.**
   It's structured, free, and needs no browser.
3. **Promote Ticketmaster from "optional secondary" to a tier-3 provider.**
   All 4 named venues and 7 of 14 overall sell through Ticketmaster,
   including all 3 resolvable venues that would otherwise need a headless
   browser. Culture Room and Hard Rock Live expose Ticketmaster event IDs on
   the page, so linking venue → Ticketmaster venue ID is straightforward.
4. **No headless browser for the MVP.** Every JS-rendered venue with listings
   is covered by Ticketmaster. Revisit only if a followed venue is
   JS-rendered and not on Ticketmaster.
5. **LLM extraction (#19) stays, but as a narrow fallback.** No named
   venue needs it once Ticketmaster is in place. Only The Amp (Pompano
   Beach) needs it today, and a CSS-selector extractor would also work
   there.
   Always filter LLM output to the target venue, since some listing pages
   span multiple venues.
6. **Some venues have no scrapable source** (Gramps, Respectable Street,
   Mizner Park's concerts). The TUI should show this as a supported state,
   not an error.

## Does Ticketmaster still add coverage worth keeping?

Yes, more than expected. It gives clean structured data for the larger
venues (Hard Rock Live, iTHINK, Fillmore, Parker, Broward Center, Culture
Room, Revolution Live). For five of them (Hard Rock Live, Culture Room,
Parker, Broward Center, Fillmore), the website alone would need an LLM or a
headless browser. It misses the smaller rooms (Funky Biscuit, Crazy Uncle Mike's,
Boca Black Box, The Amp), and the scraping tiers cover those. Ticketmaster
coverage here is inferred from ticket links on venue pages; #6/#7 should
confirm it against the Discovery API.

## Consequences

- #18 scope: ICS (with probing), JSON-LD/microdata, `__NEXT_DATA__`, and
  SeeTickets white-label markup.
- #6/#7 move up in priority; they are the path for big venues.
- #19 can be deferred behind #18 and #6/#7 without losing a named venue.
- #20 (dedupe) matters: Revolution Live and Culture Room will appear in both
  their own feed/page and Ticketmaster.
- Feeds contain past events (Revolution Live: 858 total, 33 upcoming), so
  filter by start date at ingestion.
