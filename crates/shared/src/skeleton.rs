use beet::prelude::*;
use serde::Deserialize;
use serde::Serialize;

/// The default `getFeedSkeleton` page size.
pub const DEFAULT_FEED_LIMIT: usize = 50;

/// The resolved input of a feed algo call, extracted from the
/// `getFeedSkeleton` query params by the route handler.
#[derive(Debug, Clone, Reflect)]
pub struct FeedQuery {
	/// The page size, already clamped to `1..=100`.
	pub limit: usize,
	/// The exclusive resume position from a previous page.
	pub cursor: Option<FeedCursor>,
}

impl Default for FeedQuery {
	fn default() -> Self {
		Self {
			limit: DEFAULT_FEED_LIMIT,
			cursor: None,
		}
	}
}

/// The `getFeedSkeleton` output: post uris for the caller's PDS to hydrate.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct FeedSkeleton {
	/// The pagination cursor, omitted when the page is empty.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub cursor: Option<String>,
	/// The page of posts, newest first.
	pub feed: Vec<SkeletonPost>,
}

/// A single skeleton item: the post uri, optionally with a repost reason in
/// future revisions.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct SkeletonPost {
	/// The post at uri.
	pub post: String,
}

/// A pagination cursor in the compound `{indexed_at_us}::{cid}` form the
/// reference README recommends, unique per feed item.
#[derive(Debug, Clone, PartialEq, Eq, Reflect)]
pub struct FeedCursor {
	/// The `indexed_at_us` of the last returned post.
	pub indexed_at_us: u64,
	/// The cid of the last returned post, breaking timestamp ties.
	pub cid: SmolStr,
}

impl FeedCursor {
	/// A cursor from its parts.
	pub fn new(indexed_at_us: u64, cid: impl Into<SmolStr>) -> Self {
		Self {
			indexed_at_us,
			cid: cid.into(),
		}
	}

	/// Parse the `{indexed_at_us}::{cid}` form.
	pub fn parse(cursor: &str) -> Result<Self> {
		let (time, cid) = cursor
			.split_once("::")
			.ok_or_else(|| bevyhow!("malformed cursor: {cursor}"))?;
		let indexed_at_us = time
			.parse::<u64>()
			.map_err(|_| bevyhow!("malformed cursor: {cursor}"))?;
		Self::new(indexed_at_us, cid).xok()
	}
}

impl core::fmt::Display for FeedCursor {
	fn fmt(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(formatter, "{}::{}", self.indexed_at_us, self.cid)
	}
}

impl core::str::FromStr for FeedCursor {
	type Err = BevyError;
	fn from_str(cursor: &str) -> Result<Self> { Self::parse(cursor) }
}

#[cfg(test)]
mod test {
	use crate::prelude::*;
	use beet::prelude::*;

	#[beet::test]
	fn cursor_roundtrips() {
		FeedCursor::new(1725911162329308, "bafyabc")
			.to_string()
			.xmap(|cursor| FeedCursor::parse(&cursor).unwrap())
			.xpect_eq(FeedCursor::new(1725911162329308, "bafyabc"));
	}

	#[beet::test]
	fn cursor_rejects_malformed() {
		FeedCursor::parse("123").xpect_err();
		FeedCursor::parse("abc::def").xpect_err();
	}
}
