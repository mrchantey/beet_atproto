//! An AT Protocol client for bevy applications: unauthenticated reads through
//! the public Bluesky AppView ([`AppView`](prelude::AppView)) and the
//! poll-follow helper ([`FeedFollow`](prelude::FeedFollow)).
// the harness main for `cargo test --lib`; cfg gated so a plain build does not
// need the facade's `testing` feature
#[cfg(test)]
beet::test_main!();

mod appview;

/// Exports the most commonly used items.
pub mod prelude {
	pub use crate::appview::*;
	pub use beet_atproto_shared::prelude::*;
}
