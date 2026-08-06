#![doc = include_str!("../README.md")]

#[cfg(feature = "client")]
pub use beet_atproto_client as client;
#[cfg(feature = "feed")]
pub use beet_atproto_feed as feed;
pub use beet_atproto_shared as shared;

/// Exports the most commonly used items.
pub mod prelude {
	#[cfg(feature = "client")]
	pub use crate::client::prelude::*;
	#[cfg(feature = "feed")]
	pub use crate::feed::prelude::*;
	// client/feed preludes already re-export shared's, so a direct re-export
	// only when both are off
	#[cfg(not(any(feature = "client", feature = "feed")))]
	pub use crate::shared::prelude::*;
}
