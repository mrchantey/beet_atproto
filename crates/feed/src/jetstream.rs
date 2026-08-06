use crate::prelude::*;
use beet::prelude::*;
use beet::net::prelude::sockets::Message;
use beet::net::prelude::sockets::*;

/// A parsed Jetstream event received by this [`Jetstream`] entity.
#[derive(Debug, Clone, Deref, EntityTargetEvent)]
pub struct JetstreamRecv(pub JetstreamEvent);

/// Subscribes to a Jetstream endpoint (the Bluesky JSON firehose) for the life
/// of the entity, parsing commit events and indexing posts.
///
/// Dials with backoff and redials on connection loss, resuming from the last
/// seen [`Self::cursor`] so events are not missed across reconnects. Each
/// frame:
/// 1. advances the cursor,
/// 2. triggers a [`JetstreamRecv`] on this entity,
/// 3. is routed by the [`index_posts`] observer into every [`PostFilter`] +
///    [`PostIndex`] entity: matching post creates insert, deletes remove.
///
/// The transport is the socket backend beet was built with: web-sys on
/// wasm, `tungstenite` (plus a tls feature for `wss://`) natively.
#[derive(Debug, Clone, Component, Reflect)]
#[reflect(Component)]
#[component(on_add = hook_ext::entity_hook(|entity| {
	entity.observe_any(handle_message);
	entity.observe_any(index_posts);
	entity.queue_async_local(connection_loop);
}))]
pub struct Jetstream {
	/// The websocket endpoint, defaults to a public Bluesky instance.
	pub endpoint: SmolStr,
	/// The collections to subscribe to, defaults to posts only.
	pub collections: Vec<SmolStr>,
	/// The resume position in unix microseconds, advanced per event.
	pub cursor: Option<u64>,
	/// The redial policy: 250ms doubling to a 10s ceiling, forever.
	#[reflect(ignore, default = "Jetstream::default_backoff")]
	pub backoff: Backoff,
}

impl Default for Jetstream {
	fn default() -> Self {
		Self {
			endpoint: PUBLIC_JETSTREAM_ENDPOINTS[1].into(),
			collections: vec![POST_NSID.into()],
			cursor: None,
			backoff: Self::default_backoff(),
		}
	}
}

impl Jetstream {
	/// A subscription to the given endpoint, eg a local jetstream instance.
	pub fn new(endpoint: impl Into<SmolStr>) -> Self {
		Self {
			endpoint: endpoint.into(),
			..default()
		}
	}

	fn default_backoff() -> Backoff {
		Backoff::new(
			u32::MAX,
			Duration::from_millis(250),
			Duration::from_secs(10),
		)
	}

	/// The subscribe url including collection and cursor params.
	pub fn subscribe_url(&self) -> String {
		let mut url = String::from(self.endpoint.as_str());
		let mut sep = if url.contains('?') { '&' } else { '?' };
		for collection in &self.collections {
			url.push(sep);
			url.push_str("wantedCollections=");
			url.push_str(collection);
			sep = '&';
		}
		if let Some(cursor) = self.cursor {
			url.push(sep);
			url.push_str("cursor=");
			url.push_str(&cursor.to_string());
		}
		url
	}
}

/// The connection loop: dial (with the current cursor) until connected, park
/// until closed, redial. Ends when the entity or its [`Jetstream`] goes away.
///
/// Mirrors `PersistentSocket`'s loop except the url rebuilds each dial from
/// the live component, so a reconnect resumes from the advanced cursor.
async fn connection_loop(entity: AsyncEntity) -> Result {
	// the reader task signals every connection end into this channel
	let (closed_send, closed_recv) = writer_channel::unbounded::<()>();
	entity.insert(SocketClosedNotify(closed_send)).await?;
	loop {
		// component gone (or entity despawned) ends the loop
		let config = entity.get_cloned::<Jetstream>().await?;
		let mut frames = config.backoff.iter();
		let socket = loop {
			// re-fetch per attempt: the cursor advances between dials
			let url = entity.get_cloned::<Jetstream>().await?.subscribe_url();
			match Socket::connect(url.as_str()).await {
				Ok(socket) => break socket,
				Err(err) => {
					let delay = frames
						.next()
						.and_then(|frame| frame.next_attempt)
						.ok_or_else(|| {
							bevyhow!("jetstream connect to {url} gave up: {err}")
						})?;
					debug!(
						"jetstream connect to {url} failed ({err}), retrying in {delay:?}"
					);
					time_ext::sleep(delay).await;
					// end cleanly if the scene was despawned while backing off
					if !entity.is_alive().await {
						return Ok(());
					}
				}
			}
		};
		info!("jetstream connected: {}", config.endpoint);
		entity.insert(socket).await?;
		// park until the connection dies, then go round again. The brief sleep
		// damps a hot loop against an instantly dropping server.
		closed_recv
			.recv()
			.await
			.ok_or_else(|| bevyhow!("jetstream closed channel dropped"))?;
		warn!("jetstream closed: {}, reconnecting", config.endpoint);
		time_ext::sleep(*config.backoff.min()).await;
	}
}

/// Parses socket frames into [`JetstreamRecv`] triggers, advancing the cursor.
fn handle_message(
	ev: On<MessageRecv>,
	mut query: Query<&mut Jetstream>,
	mut commands: Commands,
) {
	let Message::Text(text) = ev.event().inner() else {
		return;
	};
	let event = match serde_json::from_str::<JetstreamEvent>(text) {
		Ok(event) => event,
		Err(err) => {
			warn!("jetstream skipped an unparseable event: {err}");
			return;
		}
	};
	if let Ok(mut jetstream) = query.get_mut(ev.target()) {
		jetstream.cursor = Some(event.time_us);
	}
	commands
		.entity(ev.target())
		.trigger_target(JetstreamRecv(event));
}

/// Routes commit events into every feed store: matching post creates are
/// indexed, post deletes removed, updates ignored (reference parity).
///
/// Attached by [`Jetstream`] on add; public so custom setups can attach it to
/// their own event source.
pub fn index_posts(
	ev: On<JetstreamRecv>,
	mut feeds: Query<(&PostFilter, &mut PostIndex)>,
) {
	let event = &ev.event().0;
	let Some(commit) = &event.commit else {
		return;
	};
	if commit.collection != POST_NSID {
		return;
	}
	let Some(uri) = event.at_uri() else {
		return;
	};
	let uri = SmolStr::from(uri.to_string());
	match commit.operation {
		CommitOperation::Create => {
			let Some(post) = event.post_record() else {
				return;
			};
			let Some(cid) = commit.cid.clone() else {
				return;
			};
			let indexed = IndexedPost {
				uri,
				cid,
				indexed_at_us: event.time_us,
			};
			for (_, mut index) in feeds
				.iter_mut()
				.filter(|(filter, _)| filter.matches(event, &post))
			{
				index.insert(indexed.clone());
			}
		}
		CommitOperation::Delete => {
			for (_, mut index) in feeds.iter_mut() {
				index.remove(&uri);
			}
		}
		CommitOperation::Update => {}
	}
}

#[cfg(test)]
mod test {
	use crate::jetstream_event::test::POST_CREATE;
	use crate::jetstream_event::test::POST_DELETE;
	use crate::prelude::*;
	use beet::prelude::*;
	use beet::net::prelude::sockets::Message;
	use beet::net::prelude::sockets::*;

	#[beet::test]
	fn builds_subscribe_url() {
		let mut jetstream = Jetstream::new("wss://example.com/subscribe");
		jetstream.collections =
			vec![POST_NSID.into(), "app.bsky.feed.like".into()];
		jetstream.cursor = Some(123);
		jetstream.subscribe_url().xpect_eq(
			"wss://example.com/subscribe?wantedCollections=app.bsky.feed.post&wantedCollections=app.bsky.feed.like&cursor=123",
		);
	}

	/// Inserting `Jetstream` in a test world starts the dial loop; without a
	/// socket backend `Socket::connect` fails instantly and the loop backoff
	/// idles until the world drops, which is harmless here.
	#[beet::test]
	async fn advances_cursor() {
		let mut world = AsyncPlugin::world();
		let entity = world.spawn(Jetstream::default()).flush();
		world
			.entity_mut(entity)
			.trigger_target(MessageRecv(Message::text(POST_CREATE)));
		world
			.get::<Jetstream>(entity)
			.unwrap()
			.cursor
			.xpect_eq(Some(1725911162329308));
	}

	#[beet::test]
	async fn indexes_creates_and_deletes() {
		let mut world = AsyncPlugin::world();
		let matching = world
			.spawn((PostFilter::text_contains("alf"), PostIndex::default()))
			.id();
		let other = world
			.spawn((PostFilter::text_contains("zzz"), PostIndex::default()))
			.id();
		let source = world.spawn_empty().observe_any(index_posts).id();
		world.flush();

		let create =
			serde_json::from_str::<JetstreamEvent>(POST_CREATE).unwrap();
		world
			.entity_mut(source)
			.trigger_target(JetstreamRecv(create));
		world.get::<PostIndex>(matching).unwrap().len().xpect_eq(1);
		world.get::<PostIndex>(other).unwrap().len().xpect_eq(0);

		let delete =
			serde_json::from_str::<JetstreamEvent>(POST_DELETE).unwrap();
		world
			.entity_mut(source)
			.trigger_target(JetstreamRecv(delete));
		world.get::<PostIndex>(matching).unwrap().len().xpect_eq(0);
	}
}
