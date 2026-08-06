//! An AT Protocol feed generator toolkit, the beet equivalent of the official
//! Bluesky [feed-generator](https://github.com/bluesky-social/feed-generator)
//! starter kit: Jetstream ingestion, the feed skeleton xrpc routes and the
//! feed record publish flow. See the repository README for the full picture.
// the harness main for `cargo test --lib`; cfg gated so a plain build does not
// need the facade's `testing` feature
#[cfg(test)]
beet::test_main!();

mod feed;
mod feed_routes;
mod jetstream;
mod jetstream_event;
mod post_index;
mod publish;

/// Exports the most commonly used items.
pub mod prelude {
	pub use crate::feed::*;
	pub use crate::feed_routes::*;
	pub use crate::jetstream::*;
	pub use crate::jetstream_event::*;
	pub use crate::post_index::*;
	pub use crate::publish::*;
	pub use beet_atproto_shared::prelude::*;
}
