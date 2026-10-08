# grpy

Find upcoming concerts at venues near you, pick the ones you care about in a
terminal UI, and push them to Google Calendar with a link to the show.

## Usage

Tell grpy where to look for shows:

```sh
grpy --near "Pompano Beach, FL"      # city, address or zip code
grpy --lat 26.2379 --lon -80.1248    # exact coordinates
```

Without these flags grpy uses the home location from the config file. grpy
prints the location it is using and where it came from. Place names
are looked up with [OpenStreetMap Nominatim](https://nominatim.org/).

grpy then opens a terminal UI listing venues within the configured radius.
Real venue sources aren't wired up yet, so for now the list holds made-up
demo venues near your location.

| Key | Action |
|---|---|
| `Tab` / `Shift-Tab` | Next / previous screen |
| `1`, `2` | Go to Venues, Concerts |
| `r` | Reload venues |
| `?` | Toggle key help |
| `Esc` | Close help |
| `q`, `Ctrl-C` | Quit |

## Configuration

grpy reads `$XDG_CONFIG_HOME/grpy/config.toml` (default
`~/.config/grpy/config.toml`) and keeps mutable state (followed venues,
cached shows and which shows are already on your calendar) in an SQLite
database at `$XDG_DATA_HOME/grpy/grpy.db` (default
`~/.local/share/grpy/grpy.db`). On first run it writes a commented example
config, readable only by you, and exits; set your home location there and
run it again.

| Key | Required | Default | Notes |
|---|---|---|---|
| `home.address` | one of these | | Address, city or ZIP code |
| `home.lat`, `home.lon` | one of these | | Decimal degrees; win over `address` |
| `home.radius_miles` | no | `25` | Search radius |
| `providers.ticketmaster_key` | no | | Without it, Ticketmaster-ticketed venues aren't covered (grpy warns) |
| `calendar.calendar_id` | no | `primary` | Google Calendar to add events to |
| `google.client_id`, `google.client_secret` | both or neither | | OAuth client for `grpy auth google` (see below) |
| `cache.ttl_hours` | no | `12` | How long fetched shows are reused before a venue is checked again |

`GRPY_TICKETMASTER_KEY` overrides `providers.ticketmaster_key`, and
`GRPY_GOOGLE_CLIENT_ID` / `GRPY_GOOGLE_CLIENT_SECRET` override the Google
client. grpy never prints secrets, including in config errors.

## Google Calendar sign-in

grpy adds events through your own Google Cloud OAuth client and asks only
for the `calendar.events` scope: it can view and edit events, but not
calendar settings or sharing. Create the client once:

1. In the [Google Cloud console](https://console.cloud.google.com/), create
   a project (or pick an existing one).
2. Under **APIs & Services → Library**, enable the **Google Calendar API**.
3. Under **Google Auth Platform**, set up the consent screen: any app name,
   audience **External**, and your email as support and developer contact.
   On **Audience**, add your Google account as a test user. On **Data
   access**, add the scope
   `https://www.googleapis.com/auth/calendar.events`.
4. Under **Clients**, create a client with application type **Desktop
   app**. Copy its client ID and client secret into `config.toml`:

   ```toml
   [google]
   client_id = "1234-abc.apps.googleusercontent.com"
   client_secret = "GOCSPX-..."
   ```

5. Run the sign-in:

   ```sh
   grpy auth google
   ```

   grpy opens Google's consent page in your browser (or prints the URL),
   receives the answer on a one-time `http://127.0.0.1:<port>` listener,
   and prints where it saved the refresh token. The flow uses PKCE, so the
   authorization code is useless to anyone else.

While the consent screen's publishing status is **Testing**, Google expires
refresh tokens after 7 days. To stay signed in, set it to **In production**
on the **Audience** page. A personal, unverified app works fine; Google just
warns that the app isn't verified when you sign in.

The refresh token is stored in the OS keyring (service `grpy`, account
`google-refresh-token`). Without a keyring, as on a headless machine, it
goes to `$XDG_DATA_HOME/grpy/google-refresh-token`, readable only by you.
grpy refreshes access tokens automatically. If the refresh token expires or
you revoke grpy's access at
<https://myaccount.google.com/permissions>, grpy tells you to re-run
`grpy auth google`.

## Development

```sh
nix develop        # or: direnv allow
cargo test
nix build          # build the package
nix run            # run it
```

Tests never touch the network; provider tests read recorded responses
from `tests/fixtures/`. A live Ticketmaster smoke test is ignored by
default. Run it by hand with a real key:

```sh
GRPY_TICKETMASTER_KEY=... cargo test --test ticketmaster_live -- --ignored --nocapture
```

It checks that the Ticketmaster venues from the data-source spike can be
found and prints their Ticketmaster venue IDs.

Planning and work tracking live in GitHub Issues.
