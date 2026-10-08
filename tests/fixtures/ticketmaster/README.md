# Ticketmaster Discovery API fixtures

Responses shaped like `/discovery/v2/venues` and `/discovery/v2/events/{id}`,
trimmed to the fields grpy reads plus a few it ignores. They were written by
hand from the Discovery API documentation because no API key was available
when they were made. Venue names and coordinates are real; the iTHINK
(`KovZpZAEkvEA`) and Fillmore (`KovZpZAEkedA`) IDs are real, the other IDs
are placeholders ending in `Test`.

Re-record them from the live API when a key is at hand: run the ignored
smoke test (see the README) and replace these with trimmed real responses.
