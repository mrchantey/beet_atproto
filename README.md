# beet_atproto

An [AT Protocol](https://atproto.com) (Bluesky) toolkit for the [beet](https://github.com/mrchantey/beet) engine, in the lineage of the official [feed-generator](https://github.com/bluesky-social/feed-generator) starter kit.

A feed generator is a service that answers `app.bsky.feed.getFeedSkeleton` with a list of post uris. The user's PDS resolves the feed's `at://` uri to this service via its did document, requests the skeleton, and hydrates the posts for the client. This repository provides both halves of that exchange as ordinary beet components:

- `crates/shared` (`beet_atproto_shared`): the wire types, `AtUri`, the `getFeedSkeleton` shapes and the `app.bsky.feed.post` record fields.
- `crates/client` (`beet_atproto_client`): `AppView`, unauthenticated hydrated reads through the public Bluesky AppView, and `FeedFollow`, polling a feed without repeats.
- `crates/feed` (`beet_atproto_feed`): the generator: `Jetstream` firehose ingestion into `PostFilter` + `PostIndex` feeds, the xrpc skeleton routes (`feed_generator`), and the `PublishFeed` declaration record flow.

The root `beet_atproto` crate re-exports all three behind `client`/`feed` features (both on by default), with `tungstenite`/`ureq`/`native-tls`/`rustls-tls` forwarding transports to the beet stack.

```rust,ignore
use beet::prelude::*;
use beet_atproto::prelude::*;

commands
	.spawn((HttpServer::default(), children![(
		Router::with_defaults(),
		children![(
			feed_generator(FeedGenerator::new("feed.example.com", "did:plc:me")),
			children![(
				FeedDef::new("whats-alf"),
				ChronologicalFeed,
				PostFilter::text_contains("alf"),
			)],
		)],
	)]))
	.trigger(StartRunning::from_cli);
commands.spawn(Jetstream::default());
```

## Examples

The two examples pair up: `feed_generator` serves a whats-alf feed on 8337, and `client` reads it (or Bluesky's published whats-hot when no generator is up), rendering one thread on a web page, a live terminal ui and a one-shot cli render.

```sh
cargo run --example feed_generator
cargo run --example client
```

The tutorials alongside them start at `examples/README.md`: an AT Protocol primer, then reading a feed, then running a generator.

## Testing

```sh
just test        # native suites for the three crates
just test-live   # live network tests against public Bluesky infrastructure
```
