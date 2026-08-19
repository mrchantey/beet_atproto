//! # AT Protocol Feed Client
//!
//! The reading half of the atproto pair: where `examples/feed_generator.rs`
//! *serves* a feed skeleton, this *consumes* one. It polls a feed generator for
//! post uris, hydrates them through the public Bluesky AppView, and appends
//! them to a thread.
//!
//! ## What the Demo Shows
//!
//! One thread, one router, three surfaces. The same two routes (`/` and
//! `/live`) are served to a browser, a terminal ui and a one-shot stdout
//! render, chosen by `--server` alone: no route, page or styling is written
//! twice. Every surface reads the same [`ThreadWindow`], which a background
//! poll appends to, so the demo is the *sameness* of the three outputs, not any
//! one of them.
//!
//! Note this example is `--example client`, while the generator it
//! reads from is `--example feed_generator`.
//!
//! ## Running the Example
//!
//! Each command stands alone, in a terminal of its own. With no generator up
//! the client reads Bluesky's published `whats-hot` instead, so any of these
//! fills with real posts within seconds:
//!
//! ```sh
//! # the live terminal ui (`live` is the route, so this opens on `/live`)
//! cargo run --example client -- --server=tui live
//!
//! # the ssr web page, on 8338
//! cargo run --example client
//! curl -s localhost:8338/
//!
//! # the same feed rendered once to stdout, then exit
//! cargo run --example client -- --server=cli
//! ```
//!
//! Expect a status line, then posts: a page newest first from the curl, a
//! terminal transcript growing by a block every `--poll` seconds, and a styled
//! one-shot render that exits `0`.
//!
//! To read the other half of the pair instead of `whats-hot`, run the generator
//! first and the client finds it on 8337:
//!
//! ```sh
//! cargo run --example feed_generator
//! ```
//!
//! The status line names whichever feed won, because the two are worth telling
//! apart: `following <feed> via http://localhost:8337` is the generator,
//! `via the appview` is the fallback. Naming a source (`--feed`,
//! `--skeleton-host`, `--appview-feed`) pins it and disables the fallback, so a
//! pinned generator that is down reports `cannot reach ..` and keeps retrying
//! rather than quietly reading something else.
//!
//! Every surface content-negotiates, so `--accept` re-renders any of them in
//! another format, eg `--server=cli --accept=text/html` for the raw html the
//! browser would receive.
//!
//! ## Options
//!
//! `--server` picks the surface (`http`, `tui`, `cli`), `--feed` and
//! `--skeleton-host` the generator to follow, `--appview-feed` a published feed
//! instead, `--appview` an AppView other than the public one, `--limit` the
//! posts per poll and `--poll` the seconds between polls. Any of the three
//! source flags pins the source, disabling the `whats-hot` fallback.
//!
//! ## Why Two Fetches
//!
//! A feed generator returns a *skeleton*: post uris and nothing else. The
//! Bluesky AppView normally hydrates them on the app's behalf, but it can only
//! proxy generators with a published declaration record, and a local one has
//! none. So this client does the AppView's job itself: skeleton from the
//! generator, post bodies from the public AppView.
use beet::prelude::*;
use beet_atproto::prelude::*;
// `Table::id()` on the `Thread`, shadowed in the facade prelude
use beet::thread::table::Table;

/// The feed served by `examples/feed_generator.rs` with its default publisher.
const DEFAULT_FEED: &str = "at://did:example:alice/app.bsky.feed.generator/whats-alf";
/// Bluesky's own `whats-hot`, stood in for the generator when it is not up.
/// Published, so the AppView serves it hydrated and the client needs no local
/// service at all.
const FALLBACK_FEED: &str =
	"at://did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.feed.generator/whats-hot";
/// The generator serves on 8337, so the client takes the next port.
const CLIENT_PORT: u16 = 8338;
/// How long a page waits out the first hydration (two network hops) before
/// rendering the empty-feed hint.
const FIRST_LOAD_TIMEOUT: Duration = Duration::from_secs(10);
/// The poll step while waiting for that first hydration.
const FIRST_LOAD_STEP: Duration = Duration::from_millis(250);

fn main() -> AppExit {
	App::new()
		.add_plugins(BeetPlugins)
		.init_plugin::<ThreadPlugin>()
		.init_plugin::<ThreadUiPlugin>()
		// the pages bind their status line to this by name
		.register_type::<FeedStatus>()
		.add_systems(Startup, setup)
		.run()
}

fn setup(mut commands: Commands) -> Result {
	let args = CliArgs::parse_env();
	let param = |key: &str| args.params.get(key).map(|val| val.to_string());

	// a published feed arrives pre-hydrated from the appview, ours needs the
	// skeleton hydrated locally
	let source = match param("appview-feed") {
		Some(feed) => FeedSource::AppViewFeed { feed },
		None => FeedSource::Skeleton {
			host: param("skeleton-host")
				.unwrap_or("http://localhost:8337".into()),
			feed: param("feed").unwrap_or(DEFAULT_FEED.into()),
		},
	};
	// naming a source is a request to read *that* source, so only the untouched
	// default may quietly fall back to a published feed
	let pinned = ["appview-feed", "skeleton-host", "feed"]
		.iter()
		.any(|key| param(key).is_some());
	let reader = FeedReader {
		source,
		fallback: (!pinned).then(|| FALLBACK_FEED.to_string()),
		appview: param("appview").map(AppView::new).unwrap_or_default(),
		limit: param("limit")
			.and_then(|limit| limit.parse().ok())
			.unwrap_or(30),
		poll: param("poll")
			.and_then(|poll| poll.parse().ok())
			.map(Duration::from_secs)
			.unwrap_or(Duration::from_secs(10)),
	};
	info!("following {}", reader.source.label());
	// the window is a projection of the feed, so it is spawned with the thread
	// rather than authored by the seed reduction
	commands.spawn((
		Thread::new("bluesky feed"),
		ThreadWindow::default(),
		FeedStatus::connecting(&reader.source),
		reader,
	));

	// exactly one server: a compiled-in `TuiServer` default-boots whenever
	// `--server` names nothing else, so the match must be exhaustive
	let server_bundle = match param("server")
		.map(|server| server.to_lowercase())
		.unwrap_or("http".into())
		.as_str()
	{
		"http" => HttpServer {
			port: Some(CLIENT_PORT),
			..default()
		}
		.any_bundle(),
		"tui" => TuiServer.any_bundle(),
		"cli" => CliServer::default().any_bundle(),
		_ => bevybail!("accepted --server values: http, tui, cli"),
	};
	commands
		.spawn((server_bundle, children![(
			Router::with_defaults(),
			children![routes()]
		)]))
		.trigger(StartRunning::from_cli);
	Ok(())
}

// ╔═══════════════════════════════════════════╗
// ║   Reading the feed                        ║
// ╚═══════════════════════════════════════════╝

/// Polls a feed and appends every new post into this entity's [`ThreadWindow`],
/// so the pages render a thread rather than talking to the network themselves.
#[derive(Clone, Component)]
#[component(on_add = hook_ext::entity_hook(|entity| {
	entity.queue_async_local(follow_feed);
}))]
struct FeedReader {
	source: FeedSource,
	/// A published feed to read instead when [`Self::source`] cannot be
	/// reached, so the client is a demo on its own rather than half of one.
	/// `None` once a source is named explicitly.
	fallback: Option<String>,
	appview: AppView,
	/// The page size, and so the maximum number of posts a poll can add.
	limit: usize,
	/// How long to wait between polls.
	poll: Duration,
}

/// Where the posts come from.
#[derive(Clone)]
enum FeedSource {
	/// A feed generator's skeleton, hydrated post by post through the AppView.
	Skeleton { host: String, feed: String },
	/// A published feed, hydrated by the AppView in one call.
	AppViewFeed { feed: String },
}

impl FeedSource {
	/// A one line description for the boot log and the page heading.
	fn label(&self) -> String {
		match self {
			Self::Skeleton { host, feed } => format!("{feed} via {host}"),
			Self::AppViewFeed { feed } => format!("{feed} via the appview"),
		}
	}
}

/// The reader's state in one line, bound into both pages. A feed can be empty
/// for several undramatic reasons (the generator is not up yet, no post has
/// matched its filter), and a terminal ui owns the screen the warning would
/// otherwise be logged to, so the state has to be renderable, not just logged.
#[derive(Default, Clone, Component, Reflect)]
#[reflect(Component, Default)]
struct FeedStatus {
	text: String,
}

impl FeedStatus {
	/// The state before the first poll answers.
	fn connecting(source: &FeedSource) -> Self {
		Self {
			text: format!("connecting to {}…", source.label()),
		}
	}
}

/// Poll the feed for the life of the entity, appending each new tail and
/// publishing what happened to [`FeedStatus`]. Fetch failures are reported and
/// retried: a generator that has not booted yet, or a network blip, must not
/// end the loop.
async fn follow_feed(entity: AsyncEntity) -> Result {
	let reader = entity.get_cloned::<FeedReader>().await?;
	// the live source, which an unreachable generator may replace once
	let mut source = reader.source.clone();
	let mut fallback = reader.fallback.clone();
	let mut follow = FeedFollow::default();
	// set when this pass swapped the source, so the replacement is read at once
	// rather than after an idle poll interval
	let mut switched = false;
	loop {
		let status = match fetch_new_posts(&reader, &source, &mut follow).await {
			// connected, but a feed with no matching post yet looks identical to
			// a broken one on screen, so the two are spelled out differently
			Ok(posts) => match append_posts(&entity, posts).await? {
				0 => format!(
					"connected to {}, no matching post yet. Is the generator \
					 ingesting, and is its --filter common enough?",
					source.label()
				),
				total => format!("following {} · {total} posts", source.label()),
			},
			Err(err) => {
				warn!("feed fetch failed: {err}");
				match fallback.take() {
					// no generator, so read a published feed instead rather than
					// leave the demo staring at an empty thread
					Some(feed) => {
						source = FeedSource::AppViewFeed { feed };
						switched = true;
						info!("generator unreachable, falling back to {}", source.label());
						format!(
							"no generator at {}, reading {} instead. Pass --feed to \
							 insist on a generator.",
							reader.source.label(),
							source.label()
						)
					}
					None => format!(
						"cannot reach {}, retrying every {:?}: {}",
						source.label(),
						reader.poll,
						// a transport error renders with a trailing newline
						err.to_string().trim()
					),
				}
			}
		};
		set_status(&entity, status).await?;
		if !std::mem::take(&mut switched) {
			time_ext::sleep(reader.poll).await;
		}
		if !entity.is_alive().await {
			return Ok(());
		}
	}
}

/// Publish the reader's state for the pages to render.
async fn set_status(entity: &AsyncEntity, text: String) -> Result {
	entity
		.with(move |mut entity| {
			if let Some(mut status) = entity.get_mut::<FeedStatus>() {
				status.text = text;
			}
		})
		.await?;
	Ok(())
}

/// The posts added since the last poll, oldest first. `source` is passed
/// separately from `reader`, which holds only the configured one.
async fn fetch_new_posts(
	reader: &FeedReader,
	source: &FeedSource,
	follow: &mut FeedFollow,
) -> Result<Vec<AppViewPost>> {
	match source {
		FeedSource::Skeleton { host, feed } => {
			let skeleton = Request::get(format!(
				"{host}/xrpc/app.bsky.feed.getFeedSkeleton?feed={feed}&limit={}",
				reader.limit
			))
			.send()
			.await?
			.into_result()
			.await?
			.json::<FeedSkeleton>()
			.await?;
			// only the uris we have not shown are worth hydrating
			let uris =
				follow.unseen(skeleton.feed.into_iter().map(|post| post.post));
			reader.appview.get_posts(&uris).await
		}
		FeedSource::AppViewFeed { feed } => {
			let page = reader.appview.get_feed(feed, reader.limit, None).await?;
			// already hydrated, so dedup keeps the post beside its uri
			let unseen = follow
				.unseen(page.feed.iter().map(|item| item.post.uri.clone()))
				.into_iter()
				.collect::<HashSet<_>>();
			page.feed
				.into_iter()
				.rev()
				.map(|item| item.post)
				.filter(|post| unseen.contains(&post.uri))
				.collect::<Vec<_>>()
				.xok()
		}
	}
}

/// Append hydrated posts to the thread window, oldest first so window order
/// stays chronological. Each author becomes an [`Actor`], resolved by did so a
/// prolific poster keeps one identity across polls. Returns the window's total,
/// which the status line reads to tell an empty feed from a working one.
async fn append_posts(
	entity: &AsyncEntity,
	posts: Vec<AppViewPost>,
) -> Result<usize> {
	let count = posts.len();
	let total = entity
		.with(move |mut entity| -> Result<usize> {
			let thread = entity
				.get::<Thread>()
				.ok_or_else(|| bevyhow!("feed reader is not on a thread"))?
				.id();
			let mut window = entity
				.get_mut::<ThreadWindow>()
				.ok_or_else(|| bevyhow!("feed thread has no window"))?;
			for post in posts {
				let author = resolve_actor(&mut window, &post.author);
				let mut appended = AgentPost::new_text(
					author,
					thread,
					post.record.text,
					PostStatus::Completed,
				);
				let metadata = appended.metadata_mut();
				metadata.insert("uri", post.uri.as_str());
				metadata.insert("handle", post.author.handle.as_str());
				metadata.insert("likes", post.like_count);
				metadata.insert("reposts", post.repost_count);
				metadata.insert("created_at", post.record.created_at);
				window.upsert_post(appended);
			}
			window.post_views().count().xok()
		})
		.await??;
	if count > 0 {
		info!("appended {count} posts");
	}
	Ok(total)
}

/// The window's actor for this account, minted on first sight. Actors are keyed
/// by did rather than handle, the only account identity that never changes.
fn resolve_actor(window: &mut ThreadWindow, author: &ProfileViewBasic) -> ActorId {
	window
		.actors()
		.iter()
		.find(|(_, actor)| {
			actor.metadata().get("did").and_then(|did| did.as_str()).ok()
				== Some(author.did.as_str())
		})
		.map(|(id, _)| *id)
		.unwrap_or_else(|| {
			let mut actor = Actor::new(author.label(), ActorKind::Agent);
			actor.metadata_mut().insert("did", author.did.as_str());
			window.insert_actor(actor)
		})
}

// ╔═══════════════════════════════════════════╗
// ║   Pages                                   ║
// ╚═══════════════════════════════════════════╝

fn routes() -> impl Bundle {
	(
		BaseLayout::<FeedLayout>::default(),
		children![
			render_action::async_route("", feed_page),
			render_action::system_route("live", live_page)
		],
	)
}

/// The document layout wrapping both pages. The `<head>` is non-visual, so the
/// same layout renders in a browser and a terminal.
#[template]
fn FeedLayout() -> impl Bundle {
	rsx! {
		<html>
			<head><title>"Bluesky Feed"</title></head>
			<body>
				<nav>
					<a href="/">"Feed"</a>
					" | "
					<a href="/live">"Live"</a>
				</nav>
				<main><Slot/></main>
			</body>
		</html>
	}
}

/// The feed as a static page, newest first: a plain snapshot of the window
/// built fresh per request, so a one-shot render (curl, `--server=cli`) shows
/// the posts that have arrived so far.
async fn feed_page(cx: ActionContext<Request>) -> impl Bundle {
	let rows = feed_rows(cx.caller.world()).await;
	// the reader already knows why a feed is empty, so the page reports its
	// state rather than guessing at one
	let status = feed_status(cx.caller.world()).await;
	rsx! {
		<div>
			<h1>"Bluesky Feed"</h1>
			<p {meta_line()}>{status}</p>
			<p>{format!("{} posts, newest first", rows.len())}</p>
			{rows.into_iter().map(post_row).collect::<Vec<_>>()}
		</div>
	}
}

/// The reactive surface: a [`ThreadView`] bound to the feed thread, repainting
/// as posts stream in, above it a status line bound to [`FeedStatus`] so a
/// transcript that stays empty says why. Only a live surface drives the
/// repaints, so a snapshot surface (curl, `--server=cli`) renders the status
/// and an empty transcript.
fn live_page(
	_cx: In<ActionContext<Request>>,
	threads: Query<(Entity, &FeedStatus), With<Thread>>,
) -> impl Bundle + use<> {
	let thread = threads.iter().next();
	// seeded with the state at build time so a snapshot surface renders the
	// truth, bound so a live one keeps it current
	let status = thread.map(|(thread, status)| {
		rsx! {
			<p {meta_line()}>{(
				Value::new(status.text.as_str()),
				ReflectFieldRef::new("FeedStatus", "text").with_target(thread),
			)}</p>
		}
	});
	let view = thread.map(|thread| {
		rsx! { <div {(ThreadView, OfThread(thread.0))}/> }
	});
	let missing = thread
		.is_none()
		.then(|| rsx! { <p>"No feed thread in this app."</p> });
	rsx! {
		<div {live_column()}>{status}{view}{missing}</div>
	}
}

/// One post: the author line, then the account and engagement line, then the
/// body.
fn post_row(row: FeedRow) -> impl Bundle {
	rsx! {
		<div {row_block()}>
			<div {author_line()}>{row.author}</div>
			<div {meta_line()}>{row.meta}</div>
			<div>{row.text}</div>
		</div>
	}
}

/// Snapshot the feed rows, waiting out the first hydration (two network hops
/// behind boot) so the first request does not render an empty page.
async fn feed_rows(world: &AsyncWorld) -> Vec<FeedRow> {
	let mut waited = Duration::ZERO;
	loop {
		let rows = world
			.with_state::<Query<&ThreadWindow>, Vec<FeedRow>>(|windows| {
				windows
					.iter()
					.next()
					.map(FeedRow::newest_first)
					.unwrap_or_default()
			})
			.await;
		if !rows.is_empty() || waited >= FIRST_LOAD_TIMEOUT {
			return rows;
		}
		time_ext::sleep(FIRST_LOAD_STEP).await;
		waited += FIRST_LOAD_STEP;
	}
}

/// The reader's current status line, snapshotted for a static page.
async fn feed_status(world: &AsyncWorld) -> String {
	world
		.with_state::<Query<&FeedStatus>, String>(|statuses| {
			statuses
				.iter()
				.next()
				.map(|status| status.text.clone())
				.unwrap_or_default()
		})
		.await
}

/// One post's rendered fields, snapshotted out of the window so the page builds
/// without holding a world borrow.
struct FeedRow {
	author: String,
	meta: String,
	text: String,
}

impl FeedRow {
	/// The window's posts as rows, newest first (window order is
	/// chronological).
	fn newest_first(window: &ThreadWindow) -> Vec<Self> {
		let mut rows = window.post_views().map(Self::new).collect::<Vec<_>>();
		rows.reverse();
		rows
	}

	fn new(view: PostView) -> Self {
		let meta = |key: &str| {
			view.post
				.metadata()
				.get(key)
				.map(|value| value.to_string())
				.unwrap_or_default()
		};
		Self {
			author: view.actor.name().to_string(),
			meta: format!(
				"@{} · {} likes · {} reposts · {}",
				meta("handle"),
				meta("likes"),
				meta("reposts"),
				meta("created_at")
			),
			text: view.post.body_str().unwrap_or_default().to_string(),
		}
	}
}

// ╔═══════════════════════════════════════════╗
// ║   Styling                                 ║
// ╚═══════════════════════════════════════════╝

/// A full-height column, so the live transcript scrolls inside the page rather
/// than growing it.
fn live_column() -> OnSpawn {
	inline_class![
		(common_props::DisplayProp, style::Display::Flex),
		(common_props::FlexDirectionProp, style::Direction::Vertical),
		(common_props::Height, Length::ViewportHeight(100.)),
	]
}

/// One post block, spaced off its neighbours.
fn row_block() -> OnSpawn {
	inline_class![(
		common_props::MarginProp,
		style::Spacing {
			bottom: Length::Rem(1.),
			..default()
		}
	)]
}

/// The author line, the post's loudest field.
fn author_line() -> OnSpawn {
	inline_class![(common_props::FontWeightProp, style::FontWeight::Bold)]
}

/// The account and engagement line, quieter than the body.
fn meta_line() -> OnSpawn {
	inline_class![Declaration::token(
		common_props::ForegroundColor,
		material::colors::OnSurfaceVariant
	)]
}
