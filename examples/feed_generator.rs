//! # AT Protocol Feed Generator
//!
//! The beet equivalent of the official Bluesky [feed-generator][1] starter
//! kit: a whats-alf feed ingesting live posts from [Jetstream] and serving
//! the feed skeleton xrpc endpoints, plus a `publish` route replacing the
//! `publishFeedGen.ts` script.
//!
//! [1]: https://github.com/bluesky-social/feed-generator
//! [Jetstream]: https://docs.bsky.app/docs/advanced-guides/firehose
//!
//! ## What the Demo Shows
//!
//! A real feed generator, live against the real network: it holds a socket open
//! to the firehose, keeps every post matching `--filter`, and answers the three
//! xrpc calls Bluesky makes of a generator. The demo is the four probes below
//! returning the same shapes the official starter kit returns, over posts that
//! were written seconds earlier. `examples/client.rs` is the client that
//! reads this feed back.
//!
//! ## Running the Example
//!
//! ```sh
//! # serve on http://localhost:8337, ingesting live alf posts
//! cargo run --example feed_generator
//!
//! # any other substring, eg a whats-bevy feed
//! cargo run --example feed_generator -- --filter=bevy
//! ```
//!
//! Wait for `jetstream connected`, then probe it from another terminal:
//! ```sh
//! curl -s localhost:8337/xrpc/_health
//! curl -s localhost:8337/.well-known/did.json
//! curl -s localhost:8337/xrpc/app.bsky.feed.describeFeedGenerator
//! curl -s 'localhost:8337/xrpc/app.bsky.feed.getFeedSkeleton?feed=at://did:plc:pppppppppppppppppppppppp/app.bsky.feed.generator/whats-alf&limit=5'
//! ```
//!
//! In order: the crate version, the `did:web` document naming this service, the
//! one feed it declares, and the feed itself as a *skeleton*, `limit` post uris
//! and a cursor. `--filter` is a case insensitive *substring* match, so the
//! default `alf` fills within seconds off *half* and *Ralf*; a rarer one leaves
//! `feed` empty until a matching post crosses the firehose. The uris are the
//! client's input, and resolve in a browser via [bsky.app]'s post viewer.
//!
//! Errors follow the xrpc spec, so a feed uri this generator does not serve is
//! an `UnsupportedAlgorithm` 400 rather than an empty feed.
//!
//! [bsky.app]: https://bsky.app
//!
//! ## Options
//!
//! `--filter` is the substring a post must contain, `--hostname` the public
//! host serving the feed (the `did:web` identity in `did.json`), `--publisher`
//! the account the feed uri belongs to, `--endpoint` a Jetstream other than the
//! default, and `--server` the surface (`http` to serve, `cli` for a one-shot
//! such as `publish`). Each is declared, so `--server=cli --help` lists them
//! beside `publish`'s own, and a one-shot refuses a flag nothing declares:
//! `--hostnme` answers `did you mean --hostname?` rather than publishing a feed
//! for `example.com`.
//!
//! ## Going Live
//!
//! A real deployment serves https at a public hostname (the did:web identity),
//! then publishes the feed record to your account once:
//! ```sh
//! cargo run --example feed_generator -- \
//! 	--server=cli --hostname=feed.example.com --publisher=<your-did> \
//! 	publish --handle=<you>.bsky.social --password=<app-password> \
//! 	--display-name="What's Alf" --description="posts about alf"
//! ```
//! The feed then appears in the Bluesky app as
//! `at://{publisher}/app.bsky.feed.generator/whats-alf`. Use an app password
//! from Settings > Privacy and Security, never your account password.
use beet::prelude::*;
use beet_atproto::prelude::*;

fn main() -> AppExit {
	App::new()
		.add_plugins(BeetPlugins)
		.register_type::<GeneratorLaunch>()
		.add_systems(Startup, setup)
		.run()
}

fn setup(mut commands: Commands) -> Result {
	let args = CliArgs::parse_env();
	let launch = args.params.parse_reflect::<GeneratorParams>()?;
	let hostname = launch.hostname.unwrap_or("example.com".into());
	// parsed up front, so a malformed did fails the launch rather than every
	// feed request
	let publisher = Did::parse(
		&launch
			.publisher
			.unwrap_or("did:plc:pppppppppppppppppppppppp".into()),
	)?;
	let filter = launch.filter.unwrap_or("alf".into());
	// `--server` is a launch knob, which a `CliServer` declares
	let server = args
		.params
		.get("server")
		.map(|val: &SmolStr| val.to_lowercase())
		.unwrap_or("http".into());

	// the server owns the boot, dispatch rides its router child. A one-shot
	// refuses a flag nothing declares, a page tolerates whatever a link carries
	let server_bundle = match server.as_str() {
		"http" => HttpServer::default().any_bundle(),
		"cli" => (CliServer::default(), RejectUnknownParams).any_bundle(),
		_ => bevybail!("accepted --server values: http, cli"),
	};
	// `CallOnReady` is the boot verb: on spawn it calls this entity's action with
	// the process request, which starts the server facet declared above it
	commands.spawn((
		server_bundle,
		GeneratorLaunch,
		CallOnReady::on_spawn(),
		children![(Router::with_defaults(), children![(
			feed_generator(FeedGenerator::new(&hostname, publisher.as_str())),
			children![
				(
					FeedDef::new("whats-alf"),
					ChronologicalFeed,
					PostFilter::text_contains(&filter),
				),
				route::exchange("publish", Publish),
			],
		)])],
	));

	// live ingest only while serving; a cli one-shot (eg publish) must not
	// open the firehose
	if server == "http" {
		let jetstream = launch
			.endpoint
			.map(|endpoint| Jetstream::new(endpoint.as_str()))
			.unwrap_or_default();
		info!(
			"ingesting posts containing '{filter}' from {}",
			jetstream.endpoint
		);
		commands.spawn(jetstream);
	}
	Ok(())
}

/// Declares the [`GeneratorParams`] on the server, so the request carrying
/// the same argv (a `--server=cli` one-shot's) recognises them and `--help`
/// lists them once, beneath every route.
#[derive(Default, Component, Reflect)]
#[reflect(Component, DeclaresParams)]
struct GeneratorLaunch;

impl DeclaresParams for GeneratorLaunch {
	fn declared_params() -> ParamsPartial {
		ParamsPartial::new::<GeneratorParams>()
	}
}

/// The flags configuring the generator, read once at launch.
#[derive(Reflect)]
struct GeneratorParams {
	/// The public host serving the feed, its `did:web` identity in
	/// `did.json`. Defaults to `example.com`.
	hostname: Option<String>,
	/// The did of the account the feed uri belongs to, `did:plc:..` or
	/// `did:web:..`. Defaults to a placeholder.
	publisher: Option<String>,
	/// The case insensitive substring a post must contain. Defaults to `alf`.
	filter: Option<String>,
	/// A Jetstream other than the default, ie `wss://..`.
	endpoint: Option<String>,
}

/// Publishes the whats-alf declaration record to your PDS, making the feed
/// appear in the Bluesky app. See the example header for the full command.
#[action]
#[derive(Default, Clone, Component, Reflect)]
#[reflect(Component)]
#[require(ParamsPartial = ParamsPartial::new::<PublishParams>())]
async fn Publish(cx: ActionContext<Request>) -> Result<Response> {
	let caller = cx.caller.clone();
	let params = cx.take().parse_params::<PublishParams>()?;
	// the feed identity comes from the generator config
	let feed_did = caller
		.with_state::<AncestorQuery<&FeedGenerator>, Result<SmolStr>>(
			|entity, generators| {
				generators
					.get(entity)
					.map(|config| config.service_did.clone())
			},
		)
		.await??;
	let mut publish = PublishFeed::new(
		feed_did,
		params.handle,
		params.password,
		params.name.unwrap_or("whats-alf".into()),
		params.display_name.unwrap_or("What's Alf".into()),
	);
	if let Some(description) = params.description {
		publish = publish.with_description(description);
	}
	if let Some(service) = params.service {
		publish = publish.with_service(service);
	}
	let uri = publish.publish().await?;
	Response::ok_text(format!(
		"published {uri}\nthe feed now appears in the Bluesky app under Feeds"
	))
	.xok()
}

/// The flags [`Publish`] reads.
#[derive(Reflect)]
struct PublishParams {
	/// The account to publish under, ie `you.bsky.social`.
	handle: String,
	/// An app password from Settings > Privacy and Security, never the
	/// account password.
	password: String,
	/// The record key, the last segment of the feed uri. Defaults to
	/// `whats-alf`.
	name: Option<String>,
	/// The feed's name in the app. Defaults to `What's Alf`.
	display_name: Option<String>,
	/// The feed's description in the app.
	description: Option<String>,
	/// The PDS service url. Defaults to `https://bsky.social`.
	service: Option<String>,
}
