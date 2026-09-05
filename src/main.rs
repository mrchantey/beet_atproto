//! This workspace's beet binary: the stock runner plus the atproto deploy types.
//!
//! The stock `beet` binary serves any entry whose tags it can resolve, which is
//! every type beet itself registers. An entry declaring `<AtprotoHandleBlock/>`
//! names a type defined in THIS workspace, which no beet build can know, so the
//! workspace that defines it builds the binary that runs it by composing the
//! stock plugins with [`AtprotoInfraPlugin`].
//!
//! Run it through the justfile, which threads the feature flag:
//!
//! ```sh
//! just cli --main=examples/infra/custom_handle_domain.bsx --stage=prod validate
//! ```
use beet::prelude::*;
use beet_atproto::prelude::*;

fn main() -> AppExit {
	// load any local `.env` (ie CLOUDFLARE_ZONE_ID) before the app starts.
	env_ext::load_dotenv().ok();
	let mut app = App::new();
	app.add_plugins((BeetPlugins, AtprotoInfraPlugin, LaunchPlugin));
	// this binary's compiled surface, spawned before the entry loads so its
	// `<CrateCheck/>` verifies against it. The primary registration, ie the one
	// an unprefixed requirement resolves to.
	app.world_mut().spawn(
		crate_registration!({
			features: ["cli", "client", "feed", "infra"]
		})
		.with_skip_prefix(),
	);
	app.run()
}
