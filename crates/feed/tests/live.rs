//! Live network tests against the public Jetstream instances, run explicitly:
//!
//! ```sh
//! cargo test -p beet_atproto_feed --test live -- --ignored
//! ```
beet::test_main!();

use beet::prelude::*;
use beet_atproto_feed::prelude::*;

/// The public firehose delivers at least one parsed commit event.
#[ignore = "requires external network"]
#[beet::test(timeout_ms = 60_000)]
async fn jetstream_receives_live() {
	let mut app = App::new();
	app.add_plugins((MinimalPlugins, AsyncPlugin));
	let events = Store::<Vec<JetstreamEvent>>::default();
	let observed = events.clone();
	app.world_mut()
		.spawn(Jetstream::default())
		.observe_any(move |ev: On<JetstreamRecv>| {
			observed.push(ev.event().0.clone());
		});
	app_ext::update_until_timeout(
		&mut app,
		move |_| !events.is_empty(),
		Duration::from_secs(30),
	)
	.await
	.xpect_true();
}

/// Full flow: live ingest into a catch-all feed, served as a skeleton of real
/// at:// post uris through the router.
#[ignore = "requires external network"]
#[beet::test(timeout_ms = 120_000)]
async fn end_to_end_live() {
	let mut app = App::new();
	app.add_plugins((MinimalPlugins, AsyncPlugin, RouterPlugin));
	let root = app
		.world_mut()
		.spawn((Router::with_defaults(), children![(
			feed_generator(FeedGenerator::new(
				"feed.example.com",
				"did:example:alice",
			)),
			children![(
				FeedDef::new("whats-alf"),
				ChronologicalFeed,
				// catch everything so live posts land quickly
				PostFilter::all(),
			)],
		)]))
		.id();
	app.world_mut().spawn(Jetstream::default());
	app_ext::update_until_timeout(
		&mut app,
		move |world| {
			world
				.query::<&PostIndex>()
				.iter(world)
				.any(|index| index.len() >= 3)
		},
		Duration::from_secs(60),
	)
	.await
	.xpect_true();

	let skeleton = app
		.world_mut()
		.entity_mut(root)
		.exchange(Request::get(
			"xrpc/app.bsky.feed.getFeedSkeleton?feed=at://did:example:alice/app.bsky.feed.generator/whats-alf&limit=3",
		))
		.await
		.json::<FeedSkeleton>()
		.await
		.unwrap();
	skeleton.feed.len().xpect_eq(3);
	skeleton
		.feed
		.iter()
		.all(|post| post.post.starts_with("at://"))
		.xpect_true();
	skeleton.cursor.xpect_some();
}
