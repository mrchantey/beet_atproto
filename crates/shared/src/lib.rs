//! AT Protocol wire types shared by the `beet_atproto` crates: the `at://`
//! record uri, the `getFeedSkeleton` request/response shapes and the
//! `app.bsky.feed.post` record fields.
// the harness main for `cargo test --lib`; cfg gated so a plain build does not
// need the facade's `testing` feature
#[cfg(test)]
beet::test_main!();

mod at_uri;
mod post_record;
mod skeleton;

/// Exports the most commonly used items.
pub mod prelude {
	pub use crate::at_uri::*;
	pub use crate::post_record::*;
	pub use crate::skeleton::*;
}
