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
//! curl -s 'localhost:8337/xrpc/app.bsky.feed.getFeedSkeleton?feed=at://did:example:alice/app.bsky.feed.generator/whats-alf&limit=5'
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
//! such as `publish`).
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
		.add_systems(Startup, setup)
		.run()
}

fn setup(mut commands: Commands) -> Result {
	let args = CliArgs::parse_env();
	let hostname = args
		.params
		.get("hostname")
		.map(|val| val.to_string())
		.unwrap_or("example.com".into());
	let publisher = args
		.params
		.get("publisher")
		.map(|val| val.to_string())
		.unwrap_or("did:example:alice".into());
	let filter = args
		.params
		.get("filter")
		.map(|val| val.to_string())
		.unwrap_or("alf".into());
	let server = args
		.params
		.get("server")
		.map(|val: &SmolStr| val.to_lowercase())
		.unwrap_or("http".into());

	// the server owns the boot, dispatch rides its router child
	let server_bundle = match server.as_str() {
		"http" => HttpServer::default().any_bundle(),
		"cli" => CliServer::default().any_bundle(),
		_ => bevybail!("accepted --server values: http, cli"),
	};
	commands
		.spawn((server_bundle, children![(
			Router::with_defaults(),
			children![(
				feed_generator(FeedGenerator::new(&hostname, &publisher)),
				children![
					(
						FeedDef::new("whats-alf"),
						ChronologicalFeed,
						PostFilter::text_contains(&filter),
					),
					route::exchange("publish", Publish),
				],
			)],
		)]))
		.trigger(StartRunning::from_cli);

	// live ingest only while serving; a cli one-shot (eg publish) must not
	// open the firehose
	if server == "http" {
		let jetstream = args
			.params
			.get("endpoint")
			.map(|endpoint| Jetstream::new(endpoint.as_str()))
			.unwrap_or_default();
		info!("ingesting posts containing '{filter}' from {}", jetstream.endpoint);
		commands.spawn(jetstream);
	}
	Ok(())
}

/// Publishes the whats-alf declaration record to your PDS, making the feed
/// appear in the Bluesky app. See the example header for the full command.
#[action(handler_only)]
#[derive(Default, Clone, Component, Reflect)]
#[reflect(Component)]
async fn Publish(cx: ActionContext<Request>) -> Result<Response> {
	let caller = cx.caller.clone();
	let request = cx.take();
	let (Some(handle), Some(password)) =
		(request.get_param("handle"), request.get_param("password"))
	else {
		return Response::from_status_body(
			StatusCode::BAD_REQUEST,
			"usage: publish --handle=<you>.bsky.social --password=<app-password> \
			 [--name=whats-alf] [--display-name=..] [--description=..] [--service=..]",
			MediaType::Text,
		)
		.xok();
	};
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
		handle,
		password,
		request.get_param("name").unwrap_or("whats-alf"),
		request.get_param("display-name").unwrap_or("What's Alf"),
	);
	if let Some(description) = request.get_param("description") {
		publish = publish.with_description(description);
	}
	if let Some(service) = request.get_param("service") {
		publish = publish.with_service(service);
	}
	let uri = publish.publish().await?;
	Response::ok_text(format!(
		"published {uri}\nthe feed now appears in the Bluesky app under Feeds"
	))
	.xok()
}
