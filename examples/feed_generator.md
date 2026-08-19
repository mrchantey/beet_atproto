# Run a feed generator

In this tutorial we will build the service behind a custom Bluesky feed: it subscribes to the live firehose, keeps the posts it likes, and serves them as a feed skeleton over HTTP. By the end you will have a working feed you can read from the client you built in the [previous tutorial](./client.md).

Everything up to the final section runs with no Bluesky account. The last section publishes the feed so it appears in the Bluesky app, and that step alone needs an account and a public hostname.

If *skeleton*, *DID* or *Jetstream* are new, start with the [AT Protocol overview](./README.md).

## Set up the project

Create a new binary crate:

```sh
cargo new hello-feed
cd hello-feed
```

Then add beet with the HTTP server, plus this repository with the native websocket transport for the firehose and the HTTP client and TLS backend:

```toml
[dependencies]
beet = { path = "/home/pete/me/beet", default-features = false, features = ["std", "http_server"] }
beet_atproto = { path = "/home/pete/me/beet_atproto", features = ["tungstenite", "ureq", "native-tls"] }
```

## Write the generator

Open `src/main.rs` and replace its contents with this:

```rust
use beet::prelude::*;
use beet_atproto::prelude::*;

fn main() -> AppExit {
	App::new()
		.add_plugins(BeetPlugins)
		.add_systems(Startup, setup)
		.run()
}

fn setup(mut commands: Commands) {
	// the server owns the boot, the router and the feed ride underneath it
	commands
		.spawn((HttpServer::default(), children![(
			Router::with_defaults(),
			children![(
				feed_generator(FeedGenerator::new(
					"example.com",
					"did:example:alice"
				)),
				children![(
					FeedDef::new("whats-alf"),
					ChronologicalFeed,
					PostFilter::text_contains("alf"),
				)],
			)],
		)]))
		.trigger(StartRunning::from_cli);

	// the firehose, feeding every matching post into the index above
	commands.spawn(Jetstream::default());
}
```

That is the whole service. Read it from the inside out:

- `PostFilter::text_contains("alf")` decides what belongs in the feed. This is your algorithm, and it can be any closure over the event and the post record.
- `ChronologicalFeed` decides what a page looks like: newest first, cursor paginated, over the posts the filter kept. Swapping in your own means providing an `Action<FeedQuery, FeedSkeleton>` instead.
- `FeedDef::new("whats-alf")` names the feed. This is the rkey, the tail of the feed's at:// uri.
- `feed_generator(..)` attaches the four endpoints the network expects: `getFeedSkeleton`, `describeFeedGenerator`, the DID document at `/.well-known/did.json`, and a health probe.
- `Jetstream::default()` subscribes to a public firehose instance and routes post creates and deletes into every feed's index.

The `FeedGenerator::new("example.com", "did:example:alice")` arguments are the two identities involved: the hostname serving the feed (which becomes `did:web:example.com`), and the DID of the account that will publish it. Placeholders are fine while you are local; the publishing section swaps in real ones.

## Run it

```sh
cargo run
```

The process connects to the firehose and serves on port 8337. In another terminal, ask it what it is:

```sh
curl -s localhost:8337/xrpc/_health
curl -s localhost:8337/.well-known/did.json
curl -s localhost:8337/xrpc/app.bsky.feed.describeFeedGenerator
```

```text
{"version":"0.1.0"}
{"@context":["https://www.w3.org/ns/did/v1"],"id":"did:web:example.com","service":[{"id":"#bsky_fg","serviceEndpoint":"https://example.com","type":"BskyFeedGenerator"}]}
{"did":"did:web:example.com","feeds":[{"uri":"at://did:example:alice/app.bsky.feed.generator/whats-alf"}]}
```

Then ask for the feed itself:

```sh
curl -s 'localhost:8337/xrpc/app.bsky.feed.getFeedSkeleton?feed=at://did:example:alice/app.bsky.feed.generator/whats-alf&limit=3'
```

```text
{"cursor":"1785896267179618::bafyreia7sodws7p4ab...","feed":[{"post":"at://did:plc:7kvekkdjojz3vbcqko6w6hup/app.bsky.feed.post/3msce352qwc2n"},...]}
```

Real accounts, real posts, seconds old. Note what is *not* in that response: no text, no authors, no counts. A skeleton is uris and a cursor, and that is the entire contract.

Pass that cursor back as `&cursor=...` and you get the next page, with no overlap. Ask for a feed that does not exist and you get the protocol's error shape, `{"error":"UnsupportedAlgorithm",...}` with a 400, which is what the network expects when it probes you.

The feed fills within seconds, which is generous of it: `text_contains` is a plain case insensitive substring match, so `alf` also catches *half*, *Ralf* and *alfabeto*. Your algorithm is being literal, and tightening it is one edit, swapping the filter for any closure over the event and the record:

```rust
PostFilter::new(|_, post| {
	post.text
		.to_lowercase()
		.split_whitespace()
		.any(|word| word == "alf")
}),
```

Restart with that and the feed goes quiet for a long while, which is the honest whats-alf. Keep the substring filter for now, so the next step has posts to show.

## Read it with your client

Leave the generator running and go back to the [client tutorial](./client.md)'s second program, the one that fetches a skeleton and hydrates it. Its defaults already point at `http://localhost:8337` and the `whats-alf` feed, so:

```sh
cargo run
```

prints your own feed, in full, with authors and post text. That is the complete loop: your filter chose the posts, your service served the uris, and the public AppView filled in the rest. Nobody had to authenticate.

## Publish it to a real account

This last step is optional, and the only one needing an account. Publishing writes a small record to your repo saying "this feed exists and this service answers for it". Once it is there, the feed appears in the Bluesky app and the AppView will proxy it for anyone.

Two things must be true first:

1. Your generator answers **https** at a public hostname. That hostname *is* its identity, because the network resolves `did:web:yourhost.com` by fetching `https://yourhost.com/.well-known/did.json`, which your service already serves. A tunnel is plenty for a first run.
2. You have an **app password**, from Settings > Privacy and Security in the Bluesky app. Never your account password. Revoke it afterwards if you like.

Point the generator at the real hostname, restart it, and confirm `https://yourhost.com/.well-known/did.json` loads from the open internet:

```rust
				feed_generator(FeedGenerator::new(
					"yourhost.com",
					"did:plc:your-account-did"
				)),
```

Then add a second binary, `src/bin/publish.rs`:

```rust
use beet::prelude::*;
use beet_atproto::prelude::*;

#[beet::main]
async fn main() {
	let args = CliArgs::parse_env();
	let param = |key: &str| args.params.get(key).unwrap().to_string();

	let uri = PublishFeed::new(
		// the service did: who serves the feed
		format!("did:web:{}", param("hostname")),
		// the account the feed record belongs to
		param("handle"),
		param("password"),
		"whats-alf",
		"What's Alf",
	)
	.with_description("posts about alf")
	.publish()
	.await
	.unwrap();

	cross_log!("published {uri}");
}
```

`cargo run` is now ambiguous between two binaries, so name the default in `Cargo.toml`:

```toml
[package]
default-run = "hello-feed"
```

Publish once:

```sh
cargo run --bin publish -- --hostname=yourhost.com --handle=you.bsky.social --password=your-app-password
```

```text
published at://did:plc:your-account-did/app.bsky.feed.generator/whats-alf
```

Open the Bluesky app and the feed is under Feeds, served by the process on your machine. `PublishFeed` also has `unpublish`, which deletes the record and removes the feed; re-running `publish` updates the display name and description in place.

The neat closing test is to read your published feed back through the *official* path, using the first client program with your new uri in place of `DISCOVER_FEED_URI`. That request goes to the AppView, which resolves your declaration record, calls your laptop for the skeleton, hydrates it, and hands it back. Every arrow in the protocol, exercised once.

## What you have built

You have run a piece of social infrastructure. It is a filter over a firehose with four HTTP endpoints, and the reason that is enough is the protocol's central split: curation is separate from storage and indexing, so a feed generator never stores a post or serves its text.

Some directions from here:

- **A better algorithm.** `PostFilter::new` takes any closure over the event and the record, and any `Action<FeedQuery, FeedSkeleton>` can replace `ChronologicalFeed` entirely. Ranking, sampling and per-feed state are all just Rust.
- **Several feeds at once.** Add more `FeedDef` children, each with its own filter and rkey. They share the one firehose connection.
- **Persistence.** `PostIndex` is in memory, like the reference kit's default, so a restart starts the feed over. A durable store is the natural next step for anything long-lived.
