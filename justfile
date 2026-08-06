# beet_atproto workflows.

# List recipes.
default:
	@just --list

# Native tests for the three crates (the beet harness; pass `--snap` to update snapshots).
test *args:
	cargo test -p beet_atproto_shared {{args}}
	cargo test -p beet_atproto_client {{args}}
	cargo test -p beet_atproto_feed {{args}}

# Live network tests against the public Bluesky instances.
test-live:
	cargo test -p beet_atproto_client --test live -- --ignored
	cargo test -p beet_atproto_feed --test live -- --ignored

# Wasm builds of the lib crates.
build-wasm:
	cargo build --target wasm32-unknown-unknown -p beet_atproto_shared -p beet_atproto_client -p beet_atproto_feed

# Serve the whats-alf generator example on 8337.
feed-generator *args:
	cargo run --example feed_generator -- {{args}}

# Serve the client example on 8338.
client *args:
	cargo run --example client -- {{args}}
