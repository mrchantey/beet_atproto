//! This workspace's beet cli: the stock runner plus the atproto deploy types.
//!
//! The stock `beet` binary serves any entry whose tags it can resolve, which is
//! every type beet itself registers. An entry declaring `<AtprotoHandleBlock/>`
//! names a type defined in THIS workspace, which no beet build can know, so the
//! workspace that defines it builds the binary that runs it: the same
//! [`launch::app`] the stock binary uses, with [`AtprotoInfraPlugin`] linked in.
//!
//! Run it through the justfile, which threads the feature flag:
//!
//! ```sh
//! just cli --main=examples/infra/custom_handle_domain.bsx validate
//! ```
use beet::prelude::*;
use beet_atproto::prelude::*;
use beet_cli::prelude::*;

fn main() -> AppExit {
	// load any local `.env` (ie CLOUDFLARE_ZONE_ID) before the app starts.
	env_ext::load_dotenv().ok();
	let mut app = launch::app(AtprotoInfraPlugin);
	// this crate's compiled surface beside beet-cli's, so an entry may require
	// the block registrations by name: `<CrateCheck features={["beet_atproto/infra"]}/>`.
	app.world_mut().spawn(crate_registration!({
		features: ["cli", "client", "feed", "infra"]
	}));
	app.run()
}
