//! The feed generator wire types the client and the feed share: the
//! `getFeedSkeleton` request and response shapes. The protocol's own types
//! (`AtUri`, `Did`, `FeedPost`, handle resolution) are beet's.
// the harness main for `cargo test --lib`; cfg gated so a plain build does not
// need the facade's `testing` feature
#[cfg(test)]
beet::test_main!();

mod skeleton;

/// Exports the most commonly used items.
pub mod prelude {
	pub use crate::skeleton::*;
}
