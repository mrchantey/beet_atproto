# AT Protocol

These two tutorials build a Bluesky feed with beet: a client that reads one, and a generator that decides what goes in one. This page is the ground you need first. It assumes you write Rust and know nothing about atproto, and it should take about ten minutes.

Neither tutorial needs a Bluesky account until the very last step of the generator one, and that step is optional.

## The idea

The AT Protocol is an attempt to make social media work like email or the web rather than like a walled garden. The bet is that the three things a social network does can be pulled apart and run by different people:

- **storage**: your posts live in a repository you own, hosted wherever you like
- **indexing**: someone crawls all those repositories and answers questions like "who replied to this"
- **curation**: someone decides which posts you see, and in what order

Bluesky the app is one product built on top. Bluesky the company runs the biggest host and index, but the protocol is designed so that none of those roles has to be them, and so that you can leave one without leaving the network. Curation is the role that is fully open today: anyone can run a feed, and users subscribe to it from inside the app. That is what we are building.

## The pieces

You will meet these terms constantly, so here they are in one place. They are worth reading in order, because each builds on the last.

**DID** (decentralized identifier), eg `did:plc:z72i7hdynmk6r22z27h6tvur`. Your permanent account identity. It never changes, and it resolves to a document saying where your data lives and which keys sign it. There are two flavours you will see: `did:plc:...`, a hosted registry, and `did:web:example.com`, which just means "fetch `https://example.com/.well-known/did.json`". Our feed generator uses the second, because serving a JSON file is easier than registering anything.

**Handle**, eg `alice.bsky.social`. A human readable name that points at a DID, and can be a domain you own. Handles change; DIDs do not. Anything that needs to be stable keys off the DID.

**PDS** (personal data server). The host holding your repository. It stores your records and hands them to the network.

**Repository**, or repo. Your account's data, a signed content addressed tree of records. Everything you create is a record in your repo: posts, likes, follows, and the declaration that says "I have a feed".

**Record**, addressed by an **at:// URI** like `at://did:plc:abc/app.bsky.feed.post/3l3rkey`. Read that as three parts: the repo (a DID), the collection, and the rkey (record key) within it. This is the fundamental address in atproto. A feed skeleton, later, is nothing but a list of these.

**Collection** and **NSID**. The collection is a namespaced type name: `app.bsky.feed.post` for posts, `app.bsky.feed.generator` for feed declarations. Reversed-domain style, like a Java package.

**Lexicon**. The schema language defining those types and the API methods over them. It is why the `getPosts` endpoint caps you at 25 uris per call: the lexicon says `maxLength: 25`.

**XRPC**. The wire protocol, which in practice is plain HTTP: `GET /xrpc/app.bsky.feed.getPosts?uris=...`, JSON in and JSON out. When you see an "xrpc endpoint", think "an HTTP route named after a lexicon method". Errors come back as `{"error": "InvalidRequest", "message": "..."}`, which matters when you serve them yourself.

**AppView**. The index. It crawls every repo and answers the aggregate questions a single repo cannot: what a post's text and like count are, who replied. The public instance at `https://public.api.bsky.app` answers reads with no authentication at all, which is why the client tutorial needs no account.

**Firehose** and **Jetstream**. The firehose is the live stream of every commit happening across the network. Jetstream is a friendlier JSON view of it, served over a websocket, and you can subscribe to just the collections you care about. This is how a feed generator sees new posts.

**Feed generator**. A service answering one question: given a feed and a cursor, which post uris are in it? That answer is called a **skeleton**, because it is uris only, with no post content. The client (or the AppView on its behalf) then hydrates those uris into real posts. This split is the whole design: curation is cheap and anyone can do it, because a feed generator never stores or serves post content.

## How a feed reaches a reader

Put together, opening a custom feed in the Bluesky app does this:

1. The app resolves the feed's at:// uri to a declaration record in someone's repo, which names the service DID that serves it.
2. It resolves that DID to a hostname, and asks it `GET /xrpc/app.bsky.feed.getFeedSkeleton?feed=...`.
3. Your generator answers with a page of post uris and a cursor.
4. The AppView hydrates those uris into posts with authors and counts, and the app renders them.

Your service never sees post text on the way out, and never stores a post it did not choose to index. It is a filter over the firehose with an HTTP endpoint bolted on, and in beet that is about thirty lines.

## Which tutorial first

- [Read a feed](./client.md) builds a client. No account, no keys, no server. Start here.
- [Run a feed generator](./feed_generator.md) ingests the firehose, serves a skeleton, and optionally publishes the feed to a real account.

Both use `beet_atproto`, this repository's crates, the beet equivalent of Bluesky's official [feed-generator](https://github.com/bluesky-social/feed-generator) starter kit. It is pre-release and unpublished, so a standalone project points cargo at the local checkouts by path; each tutorial's setup section shows the exact dependencies.

The `ureq` and `native-tls` features appearing there are beet's native HTTP client and TLS backend. On wasm the browser provides both, and none of the code in these tutorials changes.
