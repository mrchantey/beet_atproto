use crate::prelude::*;
use beet::exports::bevy::platform::sync::Arc;
use beet::prelude::*;
use core::cmp::Reverse;
use core::ops::Bound;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

/// A single indexed post, the row shape of a [`PostIndex`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedPost {
	/// The post at uri.
	pub uri: SmolStr,
	/// The record cid.
	pub cid: SmolStr,
	/// Index time in unix microseconds (the jetstream `time_us`).
	pub indexed_at_us: u64,
}

/// Newest first ordering key: ascending BTreeMap iteration yields the most
/// recent post first, cid breaking timestamp ties.
type PostKey = (Reverse<u64>, Reverse<SmolStr>);

impl IndexedPost {
	fn key(&self) -> PostKey {
		(Reverse(self.indexed_at_us), Reverse(self.cid.clone()))
	}

	/// The cursor pointing at this post.
	pub fn cursor(&self) -> FeedCursor {
		FeedCursor::new(self.indexed_at_us, self.cid.clone())
	}
}

/// The in-memory post index a feed serves from, the beet equivalent of the
/// reference implementation's sqlite `post` table (which also defaults to
/// `:memory:`). Ordered newest first by `(indexed_at_us, cid)` with a uri map
/// for deletes.
#[derive(Debug, Default, Clone, Component)]
pub struct PostIndex {
	posts: BTreeMap<PostKey, IndexedPost>,
	by_uri: HashMap<SmolStr, PostKey>,
}

impl PostIndex {
	/// Insert a post, replacing any existing entry for the same uri.
	pub fn insert(&mut self, post: IndexedPost) {
		self.remove(&post.uri);
		self.by_uri.insert(post.uri.clone(), post.key());
		self.posts.insert(post.key(), post);
	}

	/// Remove a post by uri, returning whether it was present.
	pub fn remove(&mut self, uri: &str) -> bool {
		self.by_uri
			.remove(uri)
			.map(|key| self.posts.remove(&key))
			.is_some()
	}

	/// The number of indexed posts.
	pub fn len(&self) -> usize { self.posts.len() }

	/// Whether the index holds no posts.
	pub fn is_empty(&self) -> bool { self.posts.is_empty() }

	/// Iterate newest first.
	pub fn iter(&self) -> impl Iterator<Item = &IndexedPost> {
		self.posts.values()
	}

	/// Page newest first, starting strictly after `cursor`.
	pub fn page(
		&self,
		limit: usize,
		cursor: Option<&FeedCursor>,
	) -> Vec<IndexedPost> {
		let start = match cursor {
			Some(cursor) => Bound::Excluded((
				Reverse(cursor.indexed_at_us),
				Reverse(cursor.cid.clone()),
			)),
			None => Bound::Unbounded,
		};
		self.posts
			.range((start, Bound::Unbounded))
			.take(limit)
			.map(|(_, post)| post.clone())
			.collect()
	}
}

/// Decides whether a post create lands in this entity's [`PostIndex`], the
/// reference implementation's indexing filter as a component.
#[derive(Clone, Component)]
pub struct PostFilter(
	Arc<dyn 'static + Send + Sync + Fn(&JetstreamEvent, &PostRecord) -> bool>,
);

impl PostFilter {
	/// A custom filter over the event and its parsed post record.
	pub fn new(
		func: impl 'static + Send + Sync + Fn(&JetstreamEvent, &PostRecord) -> bool,
	) -> Self {
		Self(Arc::new(func))
	}

	/// Index every post.
	pub fn all() -> Self { Self::new(|_, _| true) }

	/// Case insensitive text containment, the whats-alf filter shape.
	pub fn text_contains(needle: impl Into<String>) -> Self {
		let needle = needle.into().to_lowercase();
		Self::new(move |_, post| post.text.to_lowercase().contains(&needle))
	}

	/// Apply the filter.
	pub fn matches(&self, ev: &JetstreamEvent, post: &PostRecord) -> bool {
		(self.0)(ev, post)
	}
}

#[cfg(test)]
mod test {
	use crate::prelude::*;
	use beet::prelude::*;

	fn post(uri: &str, cid: &str, time: u64) -> IndexedPost {
		IndexedPost {
			uri: uri.into(),
			cid: cid.into(),
			indexed_at_us: time,
		}
	}

	fn seeded() -> PostIndex {
		let mut index = PostIndex::default();
		index.insert(post("at://a/p/1", "cid1", 100));
		index.insert(post("at://a/p/2", "cid2", 300));
		index.insert(post("at://a/p/3", "cid3", 200));
		index
	}

	#[beet::test]
	fn orders_newest_first() {
		seeded()
			.iter()
			.map(|post| post.indexed_at_us)
			.collect::<Vec<_>>()
			.xpect_eq(vec![300, 200, 100]);
	}

	#[beet::test]
	fn replaces_same_uri() {
		let mut index = seeded();
		index.insert(post("at://a/p/1", "cid1b", 400));
		index.len().xpect_eq(3);
		index.iter().next().unwrap().cid.xpect_eq("cid1b");
	}

	#[beet::test]
	fn removes() {
		let mut index = seeded();
		index.remove("at://a/p/2").xpect_true();
		index.remove("at://a/p/2").xpect_false();
		index.len().xpect_eq(2);
	}

	#[beet::test]
	fn pages_without_gaps() {
		let index = seeded();
		let first = index.page(2, None);
		first.len().xpect_eq(2);
		let cursor = first.last().unwrap().cursor();
		let second = index.page(2, Some(&cursor));
		second.len().xpect_eq(1);
		first
			.iter()
			.chain(second.iter())
			.map(|post| post.uri.clone())
			.collect::<Vec<_>>()
			.xpect_eq(vec![
				SmolStr::from("at://a/p/2"),
				SmolStr::from("at://a/p/3"),
				SmolStr::from("at://a/p/1"),
			]);
	}

	#[beet::test]
	fn filters_text() {
		let event =
			serde_json::from_str::<JetstreamEvent>(crate::jetstream_event::test::POST_CREATE)
				.unwrap();
		let post = event.post_record().unwrap();
		PostFilter::text_contains("ALF").matches(&event, &post).xpect_true();
		PostFilter::text_contains("wrong")
			.matches(&event, &post)
			.xpect_false();
		PostFilter::all().matches(&event, &post).xpect_true();
	}
}
