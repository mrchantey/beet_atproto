//! The atproto deploy blocks: `<AtprotoHandleBlock/>`, the handles it
//! publishes, and the probe that asserts they resolve.
//!
//! A custom-domain handle is one DNS TXT record. `pete.beetmash.com` is a
//! handle because `_atproto.pete.beetmash.com` holds `did=did:plc:...`, and
//! that record is the entire infrastructure: the account it names lives on
//! somebody's PDS, keeps its did across every rename, and is not declared here.
//! So the block is a zone declaration, not an account one, and the probe is the
//! only step that talks to the network at all.
//!
//! This is the first infrastructure block defined outside `beet_infra`. It owns
//! nothing beet does not: the [`Block`](beet::prelude::Block) trait, the
//! [`DeployRender`](beet::prelude::DeployRender) schedule and the generic
//! declare/render systems all come from upstream, and
//! [`AtprotoInfraPlugin`](prelude::AtprotoInfraPlugin) registers this crate's
//! type into them.
//!
//! Hosting the accounts themselves (a `PdsBlock`, provisioning, migrating an
//! existing did) is a separate question, surveyed in
//! `.agents/reports/pds-provisioning.md`.
// the harness main for `cargo test --lib`; cfg gated so a plain build does not
// need the facade's `testing` feature
#[cfg(test)]
beet::test_main!();

mod actions;
mod atproto_handle;
mod atproto_handle_block;
mod atproto_infra_plugin;

/// Exports the most commonly used items.
pub mod prelude {
	pub use crate::actions::*;
	pub use crate::atproto_handle::*;
	pub use crate::atproto_handle_block::*;
	pub use crate::atproto_infra_plugin::*;
}
