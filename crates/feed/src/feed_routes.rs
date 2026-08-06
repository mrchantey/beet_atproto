use crate::prelude::*;
use beet::prelude::*;

/// Configures a feed generator service: the public hostname the did:web
/// document points at, the service did, and the did of the account that
/// publishes the feed declaration records.
///
/// Lives on the [`feed_generator`] entity; route handlers resolve it through
/// [`AncestorQuery`], so feeds and routes nest anywhere beneath it.
#[derive(Debug, Clone, Component, Reflect)]
#[reflect(Component)]
pub struct FeedGenerator {
	/// The public hostname, eg `feed.example.com`.
	pub hostname: SmolStr,
	/// The service did, defaulting to `did:web:{hostname}`.
	pub service_did: SmolStr,
	/// The did of the account publishing the feed records, eg `did:plc:abc`.
	pub publisher_did: SmolStr,
}

impl FeedGenerator {
	/// A generator at `hostname` publishing as `publisher_did`, with the
	/// conventional `did:web:{hostname}` service did.
	pub fn new(
		hostname: impl Into<SmolStr>,
		publisher_did: impl Into<SmolStr>,
	) -> Self {
		let hostname = hostname.into();
		Self {
			service_did: format!("did:web:{hostname}").into(),
			hostname,
			publisher_did: publisher_did.into(),
		}
	}

	/// Override the service did, eg for a did:plc service.
	pub fn with_service_did(mut self, service_did: impl Into<SmolStr>) -> Self {
		self.service_did = service_did.into();
		self
	}
}

/// The feed generator service bundle: config plus the xrpc routes, placed
/// under a [`Router`](beet::router::prelude::Router). Feed entities ride beside
/// it as ordinary children:
///
/// ```rust
/// # use beet::prelude::*;
/// # use beet_atproto_feed::prelude::*;
/// let mut world = (AsyncPlugin, RouterPlugin).into_world();
/// world.spawn((default_router(), children![(
/// 	feed_generator(FeedGenerator::new("feed.example.com", "did:plc:me")),
/// 	children![(
/// 		FeedDef::new("whats-alf"),
/// 		ChronologicalFeed,
/// 		PostFilter::text_contains("alf"),
/// 	)],
/// )]));
/// ```
///
/// Routes attach as [`OnSpawn::insert_child`] effects so the `children!` feed
/// set is not clobbered.
pub fn feed_generator(config: FeedGenerator) -> impl Bundle {
	(
		config,
		OnSpawn::insert_child(GetFeedSkeleton),
		OnSpawn::insert_child(DescribeFeedGenerator),
		OnSpawn::insert_child(WellKnownDid),
		OnSpawn::insert_child(XrpcHealth),
	)
}

/// An xrpc error response: `{"error": ..., "message": ...}` with the given
/// status, the wire shape `@atproto/xrpc-server` emits.
pub fn xrpc_error(
	status: StatusCode,
	error: &str,
	message: impl AsRef<str>,
) -> Response {
	Response::from_status_body(
		status,
		serde_json::json!({"error": error, "message": message.as_ref()})
			.to_string(),
		MediaType::Json,
	)
}

/// `GET /xrpc/app.bsky.feed.getFeedSkeleton?feed=<at-uri>&limit=..&cursor=..`
///
/// Validates the feed at uri against the ancestor [`FeedGenerator`], finds the
/// matching [`FeedDef`] entity beneath it, and calls its
/// `Action<FeedQuery, FeedSkeleton>`. Caller auth is not checked, matching the
/// reference implementation's behavior for feeds that are not user specific.
#[action(handler_only, route = "xrpc/app.bsky.feed.getFeedSkeleton")]
#[derive(Debug, Default, Clone, Component, Reflect)]
#[reflect(Component, Default)]
pub async fn GetFeedSkeleton(cx: ActionContext<RequestParts>) -> Result<Response> {
	let world = cx.world();
	let caller = cx.caller.clone();
	let parts = cx.take();
	// params: feed (required at uri), limit (1..=100, default 50), cursor
	let Some(feed_param) = parts.get_param("feed") else {
		return xrpc_error(
			StatusCode::BAD_REQUEST,
			"InvalidRequest",
			"missing feed param",
		)
		.xok();
	};
	let Ok(feed_uri) = AtUri::parse(feed_param) else {
		return xrpc_error(
			StatusCode::BAD_REQUEST,
			"InvalidRequest",
			format!("invalid feed uri: {feed_param}"),
		)
		.xok();
	};
	let limit = parts
		.get_param("limit")
		.and_then(|limit| limit.parse::<usize>().ok())
		.unwrap_or(DEFAULT_FEED_LIMIT)
		.clamp(1, 100);
	let cursor = match parts.get_param("cursor") {
		Some(cursor) => match FeedCursor::parse(cursor) {
			Ok(cursor) => Some(cursor),
			Err(_) => {
				return xrpc_error(
					StatusCode::BAD_REQUEST,
					"InvalidRequest",
					"malformed cursor",
				)
				.xok();
			}
		},
		None => None,
	};
	// resolve the generator config and the addressed feed entity
	let feed_entity = caller
		.with_state::<(
			AncestorQuery<&FeedGenerator>,
			Query<(Entity, &FeedDef)>,
		), Result<Option<Entity>>>(move |entity, (generators, feeds)| {
			let generator_entity = generators.get_entity(entity)?;
			let config = generators.get(entity)?;
			if feed_uri.authority != config.publisher_did
				|| feed_uri.collection != FEED_GENERATOR_NSID
			{
				return None.xok();
			}
			feeds
				.iter()
				.filter(|(_, def)| def.rkey == feed_uri.rkey)
				.find(|(feed_entity, _)| {
					generators.get_entity(*feed_entity).ok()
						== Some(generator_entity)
				})
				.map(|(feed_entity, _)| feed_entity)
				.xok()
		})
		.await??;
	let Some(feed_entity) = feed_entity else {
		return xrpc_error(
			StatusCode::BAD_REQUEST,
			"UnsupportedAlgorithm",
			"Unsupported algorithm",
		)
		.xok();
	};
	let skeleton = world
		.entity(feed_entity)
		.call::<FeedQuery, FeedSkeleton>(FeedQuery { limit, cursor })
		.await?;
	Response::ok_json(&skeleton)
}

/// `GET /xrpc/app.bsky.feed.describeFeedGenerator`
///
/// Lists every [`FeedDef`] under this generator as its declaration uri.
#[action(handler_only, route = "xrpc/app.bsky.feed.describeFeedGenerator")]
#[derive(Debug, Default, Clone, Component, Reflect)]
#[reflect(Component, Default)]
pub fn DescribeFeedGenerator(
	cx: In<ActionContext<RequestParts>>,
	generators: AncestorQuery<&FeedGenerator>,
	feeds: Query<(Entity, &FeedDef)>,
) -> Result<Response> {
	let generator_entity = generators.get_entity(cx.id())?;
	let config = generators.get(cx.id())?;
	let feed_uris = feeds
		.iter()
		.filter(|(feed_entity, _)| {
			generators.get_entity(*feed_entity).ok() == Some(generator_entity)
		})
		.map(|(_, def)| {
			serde_json::json!({
				"uri": AtUri::feed_generator(
					config.publisher_did.clone(),
					def.rkey.clone(),
				)
				.to_string()
			})
		})
		.collect::<Vec<_>>();
	Response::ok_json(&serde_json::json!({
		"did": config.service_did,
		"feeds": feed_uris,
	}))
}

/// `GET /.well-known/did.json`
///
/// The did:web document pointing the network at this service, or 404 when the
/// service did does not end with the configured hostname (reference parity).
#[action(handler_only, route = ".well-known/did.json")]
#[derive(Debug, Default, Clone, Component, Reflect)]
#[reflect(Component, Default)]
pub fn WellKnownDid(
	cx: In<ActionContext<RequestParts>>,
	generators: AncestorQuery<&FeedGenerator>,
) -> Result<Response> {
	let config = generators.get(cx.id())?;
	if !config.service_did.ends_with(config.hostname.as_str()) {
		return Response::from_status(StatusCode::NOT_FOUND).xok();
	}
	Response::ok_json(&serde_json::json!({
		"@context": ["https://www.w3.org/ns/did/v1"],
		"id": config.service_did,
		"service": [{
			"id": "#bsky_fg",
			"type": "BskyFeedGenerator",
			"serviceEndpoint": format!("https://{}", config.hostname),
		}],
	}))
}

/// `GET /xrpc/_health`, the conventional feed generator liveness probe.
#[action(handler_only, route = "xrpc/_health")]
#[derive(Debug, Default, Clone, Component, Reflect)]
#[reflect(Component, Default)]
pub fn XrpcHealth(_cx: In<ActionContext<RequestParts>>) -> Result<Response> {
	Response::ok_json(
		&serde_json::json!({"version": env!("CARGO_PKG_VERSION")}),
	)
}

#[cfg(test)]
mod test {
	use crate::prelude::*;
	use beet::prelude::*;
	use beet::exports::bevy::ecs::system::RunSystemOnce;

	const FEED_URI: &str =
		"at://did:plc:publisher/app.bsky.feed.generator/whats-alf";

	fn test_tree() -> impl Bundle {
		(default_router(), children![(
			feed_generator(FeedGenerator::new(
				"feed.example.com",
				"did:plc:publisher",
			)),
			children![(
				FeedDef::new("whats-alf"),
				ChronologicalFeed,
				PostFilter::text_contains("alf"),
			)],
		)])
	}

	/// Spawn the tree and seed the feed's index with three posts.
	fn seeded_world() -> (World, Entity) {
		let mut world = (AsyncPlugin, RouterPlugin).into_world();
		let root = world.spawn(test_tree()).flush();
		let feed = world
			.run_system_once(
				|query: Query<Entity, With<FeedDef>>| -> Entity {
					query.single().unwrap()
				},
			)
			.unwrap();
		let mut index = world.get_mut::<PostIndex>(feed).unwrap();
		for (rkey, cid, time) in
			[("1", "cid1", 100), ("2", "cid2", 300), ("3", "cid3", 200)]
		{
			index.insert(IndexedPost {
				uri: AtUri::post("did:plc:author", rkey).to_string().into(),
				cid: cid.into(),
				indexed_at_us: time,
			});
		}
		(world, root)
	}

	#[beet::test(timeout_ms = 10000)]
	async fn serves_feed_skeleton() {
		let (mut world, root) = seeded_world();
		let response = world
			.entity_mut(root)
			.exchange(Request::get(format!(
				"xrpc/app.bsky.feed.getFeedSkeleton?feed={FEED_URI}"
			)))
			.await;
		response.status().xpect_eq(StatusCode::OK);
		let skeleton = response.json::<FeedSkeleton>().await.unwrap();
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

	#[beet::test(timeout_ms = 10000)]
	async fn paginates_with_cursor() {
		let (mut world, root) = seeded_world();
		let first = world
			.entity_mut(root)
			.exchange(Request::get(format!(
				"xrpc/app.bsky.feed.getFeedSkeleton?feed={FEED_URI}&limit=2"
			)))
			.await
			.json::<FeedSkeleton>()
			.await
			.unwrap();
		first.feed.len().xpect_eq(2);
		let second = world
			.entity_mut(root)
			.exchange(Request::get(format!(
				"xrpc/app.bsky.feed.getFeedSkeleton?feed={FEED_URI}&limit=2&cursor={}",
				first.cursor.unwrap()
			)))
			.await
			.json::<FeedSkeleton>()
			.await
			.unwrap();
		second.feed.len().xpect_eq(1);
		second.feed[0]
			.post
			.xpect_eq("at://did:plc:author/app.bsky.feed.post/1");
	}

	#[beet::test(timeout_ms = 10000)]
	async fn rejects_unsupported_algorithms() {
		let (mut world, root) = seeded_world();
		for feed in [
			// unknown rkey
			"at://did:plc:publisher/app.bsky.feed.generator/nope",
			// wrong publisher
			"at://did:plc:other/app.bsky.feed.generator/whats-alf",
			// wrong collection
			"at://did:plc:publisher/app.bsky.feed.post/whats-alf",
		] {
			let response = world
				.entity_mut(root)
				.exchange(Request::get(format!(
					"xrpc/app.bsky.feed.getFeedSkeleton?feed={feed}"
				)))
				.await;
			response.status().xpect_eq(StatusCode::BAD_REQUEST);
			response
				.text()
				.await
				.unwrap()
				.xpect_contains("UnsupportedAlgorithm");
		}
	}

	#[beet::test(timeout_ms = 10000)]
	async fn rejects_invalid_requests() {
		let (mut world, root) = seeded_world();
		let missing = world
			.entity_mut(root)
			.exchange(Request::get("xrpc/app.bsky.feed.getFeedSkeleton"))
			.await;
		missing.status().xpect_eq(StatusCode::BAD_REQUEST);
		missing
			.text()
			.await
			.unwrap()
			.xpect_contains("InvalidRequest");

		let malformed_cursor = world
			.entity_mut(root)
			.exchange(Request::get(format!(
				"xrpc/app.bsky.feed.getFeedSkeleton?feed={FEED_URI}&cursor=abc"
			)))
			.await;
		malformed_cursor.status().xpect_eq(StatusCode::BAD_REQUEST);
		malformed_cursor
			.text()
			.await
			.unwrap()
			.xpect_contains("malformed cursor");
	}

	#[beet::test(timeout_ms = 10000)]
	async fn describes_feeds() {
		let (mut world, root) = seeded_world();
		let body = world
			.entity_mut(root)
			.exchange(Request::get("xrpc/app.bsky.feed.describeFeedGenerator"))
			.await
			.unwrap_str()
			.await;
		body.xpect_contains("did:web:feed.example.com")
			.xpect_contains(FEED_URI);
	}

	#[beet::test(timeout_ms = 10000)]
	async fn serves_did_document() {
		let (mut world, root) = seeded_world();
		let body = world
			.entity_mut(root)
			.exchange(Request::get(".well-known/did.json"))
			.await
			.unwrap_str()
			.await;
		body.xpect_contains("\"id\":\"did:web:feed.example.com\"")
			.xpect_contains("BskyFeedGenerator")
			.xpect_contains("https://feed.example.com");
	}

	#[beet::test(timeout_ms = 10000)]
	async fn hides_did_document_on_foreign_service_did() {
		let mut world = (AsyncPlugin, RouterPlugin).into_world();
		let root = world
			.spawn((default_router(), children![feed_generator(
				FeedGenerator::new("feed.example.com", "did:plc:publisher")
					.with_service_did("did:web:other.com"),
			)]))
			.flush();
		world
			.entity_mut(root)
			.exchange(Request::get(".well-known/did.json"))
			.await
			.status()
			.xpect_eq(StatusCode::NOT_FOUND);
	}

	#[beet::test(timeout_ms = 10000)]
	async fn reports_health() {
		let (mut world, root) = seeded_world();
		world
			.entity_mut(root)
			.exchange(Request::get("xrpc/_health"))
			.await
			.unwrap_str()
			.await
			.xpect_contains("version");
	}
}
