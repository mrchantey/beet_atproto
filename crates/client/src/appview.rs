use crate::prelude::*;
use beet::prelude::*;
use serde::Deserialize;

/// The public unauthenticated AppView, serving hydrated reads to logged out
/// clients.
pub const PUBLIC_APPVIEW: &str = "https://public.api.bsky.app";

/// The `getPosts` lexicon caps each call at 25 uris.
pub const MAX_GET_POSTS_URIS: usize = 25;

/// Bluesky's Discover feed, a stable published generator for demos and live
/// tests of [`AppView::get_feed`].
pub const DISCOVER_FEED_URI: &str =
	"at://did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.feed.generator/whats-hot";

/// A read-only xrpc client for the Bluesky AppView, hydrating the post uris a
/// feed generator serves as a skeleton into displayable posts.
///
/// Unauthenticated: the public instance serves both calls without a session,
/// so a client needs only an http transport (eg the `ureq` and a tls feature).
#[derive(Debug, Clone)]
pub struct AppView {
	/// The AppView host, defaults to [`PUBLIC_APPVIEW`].
	pub host: String,
}

impl Default for AppView {
	fn default() -> Self { Self::new(PUBLIC_APPVIEW) }
}

impl AppView {
	/// A client against the given host, eg a self hosted AppView.
	pub fn new(host: impl Into<String>) -> Self { Self { host: host.into() } }

	/// Hydrate post uris into [`AppViewPost`]s, in the order requested.
	///
	/// Uris are chunked to [`MAX_GET_POSTS_URIS`] per call. Deleted and
	/// otherwise missing posts are omitted by the AppView, so the result may be
	/// shorter than the request.
	pub async fn get_posts(&self, uris: &[SmolStr]) -> Result<Vec<AppViewPost>> {
		let mut posts = Vec::with_capacity(uris.len());
		for chunk in uris.chunks(MAX_GET_POSTS_URIS) {
			posts.extend(
				Request::get(self.get_posts_url(chunk))
					.send()
					.await?
					.into_result()
					.await?
					.json::<GetPostsResponse>()
					.await?
					.posts,
			);
		}
		Self::order_posts(uris, posts).xok()
	}

	/// A page of a *published* feed, hydrated by the AppView in one call.
	///
	/// The AppView proxies the generator itself, so this only resolves feeds
	/// with a declaration record, erroring `UnknownFeed` otherwise. An
	/// unpublished local generator is read with `getFeedSkeleton` plus
	/// [`Self::get_posts`] instead.
	pub async fn get_feed(
		&self,
		feed: &str,
		limit: usize,
		cursor: Option<&str>,
	) -> Result<GetFeedResponse> {
		let mut url = format!(
			"{}/xrpc/app.bsky.feed.getFeed?feed={feed}&limit={limit}",
			self.host
		);
		if let Some(cursor) = cursor {
			url.push_str("&cursor=");
			url.push_str(cursor);
		}
		Request::get(url)
			.send()
			.await?
			.into_result()
			.await?
			.json::<GetFeedResponse>()
			.await
	}

	/// The `getPosts` url for one chunk, repeating the `uris` array param.
	fn get_posts_url(&self, chunk: &[SmolStr]) -> String {
		let mut url = format!("{}/xrpc/app.bsky.feed.getPosts", self.host);
		let mut sep = '?';
		for uri in chunk {
			url.push(sep);
			url.push_str("uris=");
			url.push_str(uri);
			sep = '&';
		}
		url
	}

	/// Restore the requested order, dropping uris the AppView did not return.
	fn order_posts(uris: &[SmolStr], posts: Vec<AppViewPost>) -> Vec<AppViewPost> {
		let mut by_uri = posts
			.into_iter()
			.map(|post| (post.uri.clone(), post))
			.collect::<HashMap<_, _>>();
		uris.iter().filter_map(|uri| by_uri.remove(uri)).collect()
	}
}

/// The uris of a feed already shown, so a poll appends only the new tail.
///
/// Feed skeletons and `getFeed` pages arrive newest first; polling with a fixed
/// limit re-reads posts already seen, so [`Self::unseen`] filters them out and
/// hands back the remainder oldest first, ready to append to a chronological
/// transcript.
#[derive(Debug, Default, Clone)]
pub struct FeedFollow {
	seen: HashSet<SmolStr>,
}

impl FeedFollow {
	/// The uris of `newest_first` not yet seen, returned oldest first and
	/// marked as seen.
	pub fn unseen(
		&mut self,
		newest_first: impl IntoIterator<Item = impl Into<SmolStr>>,
	) -> Vec<SmolStr> {
		newest_first
			.into_iter()
			.map(Into::into)
			.filter(|uri| self.seen.insert(uri.clone()))
			.collect::<Vec<_>>()
			.into_iter()
			.rev()
			.collect()
	}
}

/// A hydrated post: the record plus its author and engagement counts.
#[derive(Debug, Clone, Deserialize)]
pub struct AppViewPost {
	/// The post at uri.
	pub uri: SmolStr,
	/// The record cid.
	pub cid: SmolStr,
	/// The posting account.
	pub author: ProfileViewBasic,
	/// The `app.bsky.feed.post` record.
	pub record: PostRecord,
	/// The number of replies, absent unless non-zero.
	#[serde(default, rename = "replyCount")]
	pub reply_count: u64,
	/// The number of reposts, absent unless non-zero.
	#[serde(default, rename = "repostCount")]
	pub repost_count: u64,
	/// The number of likes, absent unless non-zero.
	#[serde(default, rename = "likeCount")]
	pub like_count: u64,
	/// The number of quote posts, absent unless non-zero.
	#[serde(default, rename = "quoteCount")]
	pub quote_count: u64,
	/// When the AppView indexed the post.
	#[serde(default, rename = "indexedAt")]
	pub indexed_at: String,
}

/// The account fields carried on every hydrated post.
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileViewBasic {
	/// The account did, its permanent identity.
	pub did: SmolStr,
	/// The current handle, eg `alice.bsky.social`.
	pub handle: SmolStr,
	/// The chosen display name, absent when never set.
	#[serde(default, rename = "displayName")]
	pub display_name: Option<String>,
	/// An absolute avatar url on the Bluesky cdn.
	#[serde(default)]
	pub avatar: Option<String>,
}

impl ProfileViewBasic {
	/// The name to show for this account: the display name when set, else the
	/// handle.
	pub fn label(&self) -> &str {
		self.display_name
			.as_deref()
			.filter(|name| !name.is_empty())
			.unwrap_or(&self.handle)
	}
}

/// One entry of a hydrated feed page. Reply and repost context is served
/// alongside `post` and ignored here.
#[derive(Debug, Clone, Deserialize)]
pub struct FeedViewPost {
	/// The post itself.
	pub post: AppViewPost,
}

/// The `getFeed` response: a hydrated page plus its continuation cursor.
#[derive(Debug, Clone, Deserialize)]
pub struct GetFeedResponse {
	/// The cursor for the next page, absent at the end of the feed.
	#[serde(default)]
	pub cursor: Option<String>,
	/// The page of posts, newest first.
	pub feed: Vec<FeedViewPost>,
}

/// The `getPosts` response.
#[derive(Debug, Clone, Deserialize)]
struct GetPostsResponse {
	posts: Vec<AppViewPost>,
}

#[cfg(test)]
mod test {
	use crate::prelude::*;
	use beet::prelude::*;

	/// A verbatim shaped post view: every required key plus the optional
	/// display name and counters.
	const POST_VIEW: &str = r#"{
		"uri": "at://did:plc:author/app.bsky.feed.post/3l3rkey",
		"cid": "bafyreipost",
		"author": {
			"did": "did:plc:author",
			"handle": "alice.bsky.social",
			"displayName": "Alice",
			"avatar": "https://cdn.bsky.app/img/avatar/alice.jpg"
		},
		"record": {
			"$type": "app.bsky.feed.post",
			"text": "have you seen alf lately?",
			"createdAt": "2024-09-09T19:46:02.102Z"
		},
		"replyCount": 2,
		"repostCount": 3,
		"likeCount": 4,
		"indexedAt": "2024-09-09T19:46:03.000Z"
	}"#;

	/// The same post stripped to the lexicon's required keys.
	const MINIMAL_POST_VIEW: &str = r#"{
		"uri": "at://did:plc:author/app.bsky.feed.post/3l3rkey",
		"cid": "bafyreipost",
		"author": { "did": "did:plc:author", "handle": "alice.bsky.social" },
		"record": { "$type": "app.bsky.feed.post" },
		"indexedAt": "2024-09-09T19:46:03.000Z"
	}"#;

	fn post_view(uri: &str) -> AppViewPost {
		let mut post = serde_json::from_str::<AppViewPost>(POST_VIEW).unwrap();
		post.uri = uri.into();
		post
	}

	#[beet::test]
	fn deserializes_post_view() {
		let post = serde_json::from_str::<AppViewPost>(POST_VIEW).unwrap();
		post.uri
			.xpect_eq("at://did:plc:author/app.bsky.feed.post/3l3rkey");
		post.cid.xpect_eq("bafyreipost");
		post.author.did.xpect_eq("did:plc:author");
		post.author.label().xpect_eq("Alice");
		post.record.text.xpect_eq("have you seen alf lately?");
		post.like_count.xpect_eq(4);
		post.repost_count.xpect_eq(3);
		post.reply_count.xpect_eq(2);
		// absent counters default rather than failing the whole page
		post.quote_count.xpect_eq(0);
		post.indexed_at.xpect_eq("2024-09-09T19:46:03.000Z");
	}

	#[beet::test]
	fn deserializes_minimal_post_view() {
		let post = serde_json::from_str::<AppViewPost>(MINIMAL_POST_VIEW).unwrap();
		post.like_count.xpect_eq(0);
		post.record.text.xpect_eq("");
		// no display name falls back to the handle
		post.author.label().xpect_eq("alice.bsky.social");
		post.author.avatar.xpect_none();
	}

	#[beet::test]
	fn deserializes_feed_page() {
		let page = serde_json::from_str::<GetFeedResponse>(&format!(
			r#"{{ "cursor": "1725911162329308", "feed": [{{ "post": {POST_VIEW} }}] }}"#
		))
		.unwrap();
		page.cursor.unwrap().xpect_eq("1725911162329308");
		page.feed[0].post.author.handle.xpect_eq("alice.bsky.social");
		// the end of a feed omits the cursor
		serde_json::from_str::<GetFeedResponse>(r#"{ "feed": [] }"#)
			.unwrap()
			.cursor
			.xpect_none();
	}

	#[beet::test]
	fn builds_get_posts_url() {
		AppView::default()
			.get_posts_url(&["at://a".into(), "at://b".into()])
			.xpect_eq(
				"https://public.api.bsky.app/xrpc/app.bsky.feed.getPosts?uris=at://a&uris=at://b",
			);
	}

	/// Over-long requests split into chunks of [`MAX_GET_POSTS_URIS`].
	#[beet::test]
	fn chunks_get_posts() {
		let uris = (0..MAX_GET_POSTS_URIS + 3)
			.map(|index| SmolStr::from(format!("at://{index}")))
			.collect::<Vec<_>>();
		let chunks = uris.chunks(MAX_GET_POSTS_URIS).collect::<Vec<_>>();
		chunks.len().xpect_eq(2);
		chunks[1].len().xpect_eq(3);
		AppView::default()
			.get_posts_url(chunks[1])
			.xpect_contains("uris=at://25&uris=at://26&uris=at://27")
			.xnot()
			.xpect_contains("at://24");
	}

	#[beet::test]
	fn orders_posts_and_drops_missing() {
		let uris = ["at://a".into(), "at://b".into(), "at://c".into()];
		// the appview answered out of order, and dropped the deleted `b`
		AppView::order_posts(&uris, vec![post_view("at://c"), post_view(
			"at://a",
		)])
		.iter()
		.map(|post| post.uri.as_str())
		.collect::<Vec<_>>()
		.xpect_eq(vec!["at://a", "at://c"]);
	}

	#[beet::test]
	fn follows_only_the_new_tail() {
		let mut follow = FeedFollow::default();
		// a newest-first page comes back oldest-first, ready to append
		follow
			.unseen(["at://c", "at://b", "at://a"])
			.xpect_eq(vec![
				SmolStr::from("at://a"),
				"at://b".into(),
				"at://c".into(),
			]);
		// the next poll overlaps: only the two new heads survive
		follow
			.unseen(["at://e", "at://d", "at://c", "at://b"])
			.xpect_eq(vec![SmolStr::from("at://d"), "at://e".into()]);
		// a poll with nothing new appends nothing
		follow
			.unseen(["at://e", "at://d"])
			.is_empty()
			.xpect_true();
	}
}
