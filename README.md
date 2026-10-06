# beet_atproto

An [AT Protocol](https://atproto.com) (Bluesky) toolkit for the [beet](https://github.com/mrchantey/beet) engine, in the lineage of the official [feed-generator](https://github.com/bluesky-social/feed-generator) starter kit.

A feed generator is a service that answers `app.bsky.feed.getFeedSkeleton` with a list of post uris. The user's PDS resolves the feed's `at://` uri to this service via its did document, requests the skeleton, and hydrates the posts for the client. This repository provides both halves of that exchange as ordinary beet components:

- `crates/shared` (`beet_atproto_shared`): the wire types, `AtUri`, the `getFeedSkeleton` shapes and the `app.bsky.feed.post` record fields.
- `crates/client` (`beet_atproto_client`): `AppView`, unauthenticated hydrated reads through the public Bluesky AppView, and `FeedFollow`, polling a feed without repeats.
- `crates/feed` (`beet_atproto_feed`): the generator: `Jetstream` firehose ingestion into `PostFilter` + `PostIndex` feeds, the xrpc skeleton routes (`feed_generator`), and the `PublishFeed` declaration record flow.
- `crates/infra` (`beet_atproto_infra`): the deploy blocks. `AtprotoHandleBlock` publishes the `_atproto` TXT records that make a domain you own an atproto handle, and `AtprotoHandleProbe` asserts they resolve.

The root `beet_atproto` crate re-exports all workspace crates behind `client`/`feed`/`infra` features (the first two on by default), with `tungstenite`/`ureq`/`native-tls`/`rustls-tls` forwarding transports to the beet stack. It is also a binary, behind the `cli` feature: the stock beet cli plus `AtprotoInfraPlugin`, since an entry naming a block defined here can only run from a binary that links it.

```rust,ignore
use beet::prelude::*;
use beet_atproto::prelude::*;

commands.spawn((HttpServer::default(), CallOnReady::on_spawn(), children![(
	Router::with_defaults(),
	children![(
		feed_generator(FeedGenerator::new("feed.example.com", "did:plc:me")),
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

Beet's own vocabulary is in its [glossary](https://beet.org/docs/glossary), which carries account, repo, publication, standard site document, announcement and derived file. These are the protocol's own, used throughout this workspace.

- **Rkey.** The record key a record is addressed by, the last segment of `at://<did>/<collection>/<rkey>`: 1 to 512 characters of `A-Za-z0-9._:~-`. A collection's lexicon says which kind it uses, `tid` for a record whose order matters, `nsid` for a lexicon schema, `literal:self` for a one-per-repo record like a profile. Beet mints every rkey client side on first write, so a record never exists without one and nothing waits on the PDS to mint.
- **TID.** The rkey kind every collection here uses: a 64 bit integer, one zero bit then 53 bits of microseconds since the Unix epoch and 10 bits of clock id, written as 13 base32-sortable characters (`3mw72aaeuj22n`). Sortable by creation time and collision free without coordination, since a process picks its clock id once at random and its minter never repeats a value, using the last mint plus one when the clock has not moved.
- **Strong ref.** A `{uri, cid}` pair pinning one exact version of a record, where the uri alone names whatever sits at that address now. A cid is the content hash of the record as the protocol encodes it, so a strong ref goes stale the moment the record is written again.
- **Blob.** Here a blob is content addressed rather than path keyed: `uploadBlob` answers with a `$type: "blob"` object carrying a cid, a mime type and a size, and a record embeds that object. The PDS keeps a blob only while some current record references it, so the reference is the retention and a bare cid is not one. Beet's own blob, bytes under a path, is the glossary's.
- **Natural key.** The field that identifies a record without any stored id: a publication's `url`, a standard site document's `path`, an announcement's `embed.external.uri`, a repost's `subject.uri`. It keys the index remembering which rkey each record was written at, and it is what a listing matches a record by when that index has to be rebuilt.
- **Converge.** For each declared record: read it at the rkey the index remembers, compare, then write or report, so the repo comes to match the declaration. A collection is listed only to find orphans or rebuild a lost index. The atproto twin of a tofu apply, and why running a publish twice writes nothing the second time.

## Examples

The two examples pair up: `feed_generator` serves a whats-alf feed on 8337, and `client` reads it (or Bluesky's published whats-hot when no generator is up), rendering one thread on a web page, a live terminal ui and a one-shot cli render.

```sh
cargo run --example feed_generator
cargo run --example client
```

The tutorials alongside them start at `examples/README.md`: an AT Protocol primer, then reading a feed, then running a generator.

A third example is a deploy rather than a program. `examples/infra/custom_handle_domain.bsx` points a custom-domain handle (`alice.example.com`) at a Bluesky account, which is one DNS TXT record; its header comment is the whole walkthrough, manual steps included.

```sh
just cli --main=examples/infra/custom_handle_domain.bsx --stage=prod validate
```

## Testing

```sh
just test        # native suites for the workspace crates
just test-live   # live network tests against public Bluesky infrastructure
```
