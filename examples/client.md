# Read a feed

In this tutorial we will read Bluesky feeds from Rust. By the end you will have printed a live feed to your terminal, then pointed the same client at a feed of your own.

No account, no API key, no signup. Bluesky's AppView answers reads to anyone, and that is the whole first half of this tutorial. If the words *AppView*, *skeleton* or *at:// uri* are new, read the [AT Protocol overview](./README.md) first, it takes ten minutes.

## Set up the project

These crates are pre-release and unpublished, so a standalone project points cargo at the local checkouts by path. Create a new binary crate:

```sh
cargo new hello-feed-client
cd hello-feed-client
```

Then add beet and this repository, with the native HTTP client and TLS backend:

```toml
[dependencies]
beet = { path = "/home/pete/me/beet", default-features = false, features = ["std", "net"] }
beet_atproto = { path = "/home/pete/me/beet_atproto", features = ["ureq", "native-tls"] }
```

## Read a real feed

Open `src/main.rs` and replace its contents with this:

```rust
use beet::prelude::*;
use beet_atproto::prelude::*;

#[beet::main]
async fn main() {
	let page = AppView::default()
		.get_feed(DISCOVER_FEED_URI, 10, None)
		.await
		.unwrap();

	for item in page.feed.iter().rev() {
		let post = &item.post;
		cross_log!("{} (@{})", post.author.label(), post.author.handle);
		cross_log!("{}\n", post.record.text);
	}
}
```

`AppView::default()` points at `https://public.api.bsky.app`, the public unauthenticated index. `DISCOVER_FEED_URI` is Bluesky's own Discover feed, a stable published feed handy for demos. We ask for ten posts and print them oldest first, so the newest ends up nearest your prompt.

`cross_log!` rather than `println!` because beet is cross-platform, and `println!` is silent in wasm.

## Run it

```sh
cargo run
```

After the build you will see whatever the network is talking about right now:

```text
Miss Katefabe (@misskatefabe.bsky.social)
EVERYONE STOP WHAT YOU ARE DOING and meet Pepper who weighs 2.3 lbs and is figuring
out the world for the very first time

The New York Times (@nytimes.com)
Jocelyn Benson, who has cultivated a national profile during two terms as Michigan's
secretary of state, on Tuesday won the Democratic nomination for governor.
```

Note `author.label()`: an account may have no display name, in which case it falls back to the handle. You will see both kinds of line in any real feed.

## Why that was one call, and yours will be two

`getFeed` did two jobs at once. It asked the feed's generator *which* posts are in the feed, then hydrated those uris into posts with authors and counts. The AppView can only do that for feeds with a published declaration record, though. A generator running on your laptop has none, so for your own feed you do both halves yourself:

1. ask your generator for a **skeleton**, a page of at:// uris
2. ask the AppView what those uris **are**

That is exactly what the AppView was doing on your behalf, and it is a dozen lines. Run through the [feed generator tutorial](./feed_generator.md) in another terminal first, then replace `src/main.rs` with this:

```rust
use beet::prelude::*;
use beet_atproto::prelude::*;

const SKELETON_HOST: &str = "http://localhost:8337";
const FEED: &str = "at://did:example:alice/app.bsky.feed.generator/whats-alf";

#[beet::main]
async fn main() {
	// 1. our generator says *which* posts are in the feed
	let skeleton = Request::get(format!(
		"{SKELETON_HOST}/xrpc/app.bsky.feed.getFeedSkeleton?feed={FEED}&limit=10"
	))
	.send()
	.await
	.unwrap()
	.into_result()
	.await
	.unwrap()
	.json::<FeedSkeleton>()
	.await
	.unwrap();

	// 2. the appview says what those posts *are*
	let uris = skeleton
		.feed
		.into_iter()
		.map(|post| SmolStr::from(post.post))
		.collect::<Vec<_>>();
	let posts = AppView::default().get_posts(&uris).await.unwrap();

	for post in posts.iter().rev() {
		cross_log!("{} (@{})", post.author.label(), post.author.handle);
		cross_log!("{}\n", post.record.text);
	}
}
```

```sh
cargo run
```

Same shape of output, but every post is one your own filter chose. `into_result()` is what turns an HTTP error status into a Rust error, so a generator that is not running fails loudly instead of failing to parse.

Two details worth keeping when you build on this. `get_posts` batches for you, at the 25 uris per call the lexicon allows, and it returns posts in the order you asked for them. It also silently drops posts that have since been deleted, so a hydrated page can be shorter than the skeleton that produced it. That is correct behavior, not an error to handle.

## Polling without repeats

A feed endpoint always hands back the newest page, so polling it means seeing the same posts again and again. `FeedFollow` remembers what you have shown and hands back only what is new, oldest first, ready to append to a transcript:

```rust
let mut follow = FeedFollow::default();
// inside your poll loop, before hydrating
let new_uris = follow.unseen(skeleton.feed.into_iter().map(|post| post.post));
```

## What you have built

You have read the live network twice over: once through the AppView's own curation, and once through your own. Both are unauthenticated reads of a public index, which is why neither needed an account.

The client here prints to a terminal, but nothing about it is terminal shaped. The full example beside this file, `client.rs`, feeds the same posts into a `ThreadWindow` and renders them three ways from one router: a server-rendered web page, a live terminal UI that tails new posts, and a one-shot CLI render. Run it with:

```sh
cargo run --example client -- --server=tui live
```

Next, [Run a feed generator](./feed_generator.md) builds the other half: the service that decides what belongs in a feed.
