//! Live network tests against the public Bluesky AppView, run explicitly:
//!
//! ```sh
//! cargo test -p beet_atproto_client --test live -- --ignored
//! ```
beet::test_main!();

use beet::prelude::*;
use beet_atproto_client::prelude::*;

/// The hydration path a client uses, independent of any generator: a published
/// feed page, then the same posts re-fetched by uri.
#[ignore = "requires external network"]
#[beet::test(timeout_ms = 60_000)]
async fn appview_hydrates_live() {
	let appview = AppView::default();
	let page = appview
		.get_feed(DISCOVER_FEED_URI, 3, None)
		.await
		.unwrap()
		.feed;
	// the appview omits entries it will not serve (deleted, moderated), so a
	// page is AT MOST the limit asked for rather than exactly it
	page.len().xpect_greater_than(0).xpect_less_or_equal_to(3);
	page.iter()
		.all(|item| !item.post.author.handle.is_empty())
		.xpect_true();

	let uris = page
		.iter()
		.map(|item| item.post.uri.clone())
		.collect::<Vec<_>>();
	appview
		.get_posts(&uris)
		.await
		.unwrap()
		.iter()
		.map(|post| post.uri.clone())
		.collect::<Vec<_>>()
		.xpect_eq(uris);
}
