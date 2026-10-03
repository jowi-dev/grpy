# grpy

Find upcoming concerts at venues near you, pick the ones you care about in a
terminal UI, and push them to Google Calendar with a link to the show.

## Usage

Tell grpy where to look for shows:

```sh
grpy --near "Pompano Beach, FL"      # city, address or zip code
grpy --lat 26.2379 --lon -80.1248    # exact coordinates
```

grpy prints the location it is using and where it came from. Place names
are looked up with [OpenStreetMap Nominatim](https://nominatim.org/).

## Development

```sh
nix develop        # or: direnv allow
cargo test
nix build          # build the package
nix run            # run it
```

Planning and work tracking live in GitHub Issues.
