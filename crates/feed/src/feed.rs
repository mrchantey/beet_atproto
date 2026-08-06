use crate::prelude::*;
use beet::prelude::*;

/// Declares a feed algo entity, addressable by the rkey of its declaration
/// record `at://{publisher}/app.bsky.feed.generator/{rkey}`.
///
/// Colocate an `Action<FeedQuery, FeedSkeleton>` provider (eg
/// [`ChronologicalFeed`]) supplying the algorithm, and spawn under a
/// [`feed_generator`](crate::prelude::feed_generator) entity so the routes
/// find it.
#[derive(Debug, Clone, Component, Reflect)]
#[reflect(Component)]
pub struct FeedDef {
	/// The declaration record key, eg `whats-alf`.
	pub rkey: SmolStr,
}

impl FeedDef {
	/// A feed declaration with the given rkey.
	pub fn new(rkey: impl Into<SmolStr>) -> Self { Self { rkey: rkey.into() } }
}

/// Reverse chronological page over this entity's [`PostIndex`], the reference
/// implementation's whats-alf algorithm.
#[action]
#[derive(Debug, Default, Clone, Component, Reflect)]
#[reflect(Component, Default)]
#[require(PostIndex)]
pub fn ChronologicalFeed(
	cx: In<ActionContext<FeedQuery>>,
	query: Query<&PostIndex>,
) -> Result<FeedSkeleton> {
	let index = query.get(cx.id())?;
	let posts = index.page(cx.input.limit, cx.input.cursor.as_ref());
	let cursor = posts.last().map(|post| post.cursor().to_string());
	FeedSkeleton {
		cursor,
		feed: posts
			.into_iter()
			.map(|post| SkeletonPost {
				post: post.uri.to_string(),
			})
			.collect(),
	}
	.xok()
}

#[cfg(test)]
mod test {
	use crate::prelude::*;
	use beet::prelude::*;

	fn seeded_feed(world: &mut World) -> Entity {
		let entity = world
			.spawn((FeedDef::new("test"), ChronologicalFeed))
			.flush();
		let mut index = world.get_mut::<PostIndex>(entity).unwrap();
		for (rkey, cid, time) in
			[("1", "cid1", 100), ("2", "cid2", 300), ("3", "cid3", 200)]
		{
			index.insert(IndexedPost {
				uri: AtUri::post("did:plc:author", rkey).to_string().into(),
				cid: cid.into(),
				indexed_at_us: time,
			});
		}
		entity
	}

	#[beet::test]
	async fn serves_newest_first() {
		let mut world = AsyncPlugin::world();
		let feed = seeded_feed(&mut world);
		let skeleton = world
			.entity_mut(feed)
			.call::<FeedQuery, FeedSkeleton>(default())
			.await
			.unwrap();
		skeleton
			.feed
			.iter()
			.map(|post| post.post.as_str())
			.collect::<Vec<_>>()
			.xpect_eq(vec![
				"at://did:plc:author/app.bsky.feed.post/2",
				"at://did:plc:author/app.bsky.feed.post/3",
				"at://did:plc:author/app.bsky.feed.post/1",
			]);
		skeleton.cursor.unwrap().xpect_eq("100::cid1");
	}

	#[beet::test]
	async fn paginates() {
		let mut world = AsyncPlugin::world();
		let feed = seeded_feed(&mut world);
		let first = world
			.entity_mut(feed)
			.call::<FeedQuery, FeedSkeleton>(FeedQuery {
				limit: 2,
				cursor: None,
			})
			.await
			.unwrap();
		first.feed.len().xpect_eq(2);
		let cursor = FeedCursor::parse(&first.cursor.unwrap()).unwrap();
		let second = world
			.entity_mut(feed)
			.call::<FeedQuery, FeedSkeleton>(FeedQuery {
				limit: 2,
				cursor: Some(cursor),
			})
			.await
			.unwrap();
		second.feed.len().xpect_eq(1);
		second.feed[0]
			.post
			.xpect_eq("at://did:plc:author/app.bsky.feed.post/1");
	}

	#[beet::test]
	async fn empty_feed_has_no_cursor() {
		let mut world = AsyncPlugin::world();
		let feed = world
			.spawn((FeedDef::new("empty"), ChronologicalFeed))
			.flush();
		let skeleton = world
			.entity_mut(feed)
			.call::<FeedQuery, FeedSkeleton>(default())
			.await
			.unwrap();
		skeleton.feed.is_empty().xpect_true();
		skeleton.cursor.xpect_none();
	}
}
