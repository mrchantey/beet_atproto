# beet_atproto workflows.

# `rustfmt.toml` uses nightly-only options, so formatting needs a nightly
# toolchain. Pinned to match `../beet` so both trees format identically;
# bump the two together.
fmt-toolchain := 'nightly-2026-07-02'

# List recipes.
default:
	@just --list

#     just cli --main=examples/infra/custom_handle_domain.bsx validate
#
# This workspace's beet cli: the stock runner plus `AtprotoInfraPlugin`, so an entry declaring `<AtprotoHandleBlock/>` resolves it.
cli *args:
	cargo run --features=cli -- {{args}}

# Format every workspace member with the pinned nightly. Never `cargo fmt`.
fmt *args:
	#!/usr/bin/env bash
	set -euo pipefail
	# bare `cargo fmt` on stable silently drops every nightly-only option in
	# `rustfmt.toml`, reformatting the whole tree into a huge bogus diff.
	rustup toolchain list | grep -q '^{{ fmt-toolchain }}' \
		|| rustup toolchain install {{ fmt-toolchain }} --profile minimal --component rustfmt
	cargo +{{ fmt-toolchain }} fmt --all {{ args }}

# Native tests for the four crates (the beet harness; pass `--snap` to update snapshots).
test *args:
	cargo test -p beet_atproto_shared {{args}}
	cargo test -p beet_atproto_client {{args}}
	cargo test -p beet_atproto_feed {{args}}
	cargo test -p beet_atproto_infra {{args}}

# Live network tests against the public Bluesky instances.
test-live:
	cargo test -p beet_atproto_client --test live -- --ignored
	cargo test -p beet_atproto_feed --test live -- --ignored

# Wasm builds of the lib crates.
build-wasm:
	cargo build --target wasm32-unknown-unknown -p beet_atproto_shared -p beet_atproto_client -p beet_atproto_feed -p beet_atproto_infra

# Serve the whats-alf generator example on 8337.
feed-generator *args:
	cargo run --example feed_generator -- {{args}}

# Serve the client example on 8338.
client *args:
	cargo run --example client -- {{args}}
