# beet_atproto

An [AT Protocol](https://atproto.com) (Bluesky) toolkit for the [beet](https://github.com/mrchantey/beet) engine, in the lineage of the official [feed-generator](https://github.com/bluesky-social/feed-generator) starter kit.

A feed generator is a service that answers `app.bsky.feed.getFeedSkeleton` with a list of post uris. The user's PDS resolves the feed's `at://` uri to this service via its did document, requests the skeleton, and hydrates the posts for the client. This repository provides both halves of that exchange as ordinary beet components:

- `crates/shared` (`beet_atproto_shared`): the `getFeedSkeleton` shapes the client and the feed share.
- `crates/client` (`beet_atproto_client`): `AppView`, unauthenticated hydrated reads through the public Bluesky AppView, and `FeedFollow`, polling a feed without repeats.
- `crates/feed` (`beet_atproto_feed`): the generator: `Jetstream` firehose ingestion into `PostFilter` + `PostIndex` feeds, the xrpc skeleton routes (`feed_generator`), and the `PublishFeed` declaration record flow.

The protocol's core is beet's own, each piece behind its crate's `atproto` feature: the record primitives (`Did`, `AtUri`, `Tid`, `Cid`, `StrongRef`, `BlobRef`) in `beet_core`, an account's repo as a provider (`Pds`, the converge, `XrpcPds`, handle resolution, `PostRecord`, `RichText`) in `beet_net`, and the custom-domain handle block in `beet_infra`. The root `beet_atproto` crate re-exports the workspace crates behind `client`/`feed` features (both on by default), with `tungstenite`/`ureq`/`native-tls`/`rustls-tls` forwarding transports to the beet stack.

```rust,ignore
use beet::prelude::*;
use beet_atproto::prelude::*;

commands.spawn((HttpServer::default(), CallOnReady::on_spawn(), children![(
	Router::with_defaults(),
	children![(
		feed_generator(FeedGenerator::new("feed.example.com", "did:plc:z72i7hdynmk6r22z27h6tvur")),
		children![(
			FeedDef::new("whats-alf"),
			ChronologicalFeed,
			PostFilter::text_contains("alf"),
		)],
	)],
)]));
commands.spawn(Jetstream::default());
```

## Words

Beet's own vocabulary is in its [glossary](https://beet.org/docs/glossary). The protocol's words (rkey, TID, strong ref, blob, natural key, converge) are the module docs of `beet_core::atproto` and `beet_net`'s `atproto` module, beside the code they name.

## Examples

The two examples pair up: `feed_generator` serves a whats-alf feed on 8337, and `client` reads it (or Bluesky's published whats-hot when no generator is up), rendering one thread on a web page, a live terminal ui and a one-shot cli render.

```sh
cargo run --example feed_generator
cargo run --example client
```

The tutorials alongside them start at `examples/README.md`: an AT Protocol primer, then reading a feed, then running a generator.

A custom-domain handle is a deploy rather than a program, so its walkthrough is beet's own `examples/infra/custom_handle_domain.bsx`.

## Testing

```sh
just test        # native suites for the workspace crates
just test-live   # live network tests against public Bluesky infrastructure
```
