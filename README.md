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
`~/.config/grpy/config.toml`) and keeps mutable state in
`$XDG_DATA_HOME/grpy/` (default `~/.local/share/grpy/`). On first run it
writes a commented example config, readable only by you, and exits; set
your home location there and run it again.

| Key | Required | Default | Notes |
|---|---|---|---|
| `home.address` | one of these | | Address, city or ZIP code |
| `home.lat`, `home.lon` | one of these | | Decimal degrees; win over `address` |
| `home.radius_miles` | no | `25` | Search radius |
| `providers.ticketmaster_key` | no | | Without it, Ticketmaster-ticketed venues aren't covered (grpy warns) |
| `calendar.calendar_id` | no | `primary` | Google Calendar to add events to |

`GRPY_TICKETMASTER_KEY` overrides `providers.ticketmaster_key`. grpy never
prints secrets, including in config errors.

## Development

```sh
nix develop        # or: direnv allow
cargo test
nix build          # build the package
nix run            # run it
```

Planning and work tracking live in GitHub Issues.
