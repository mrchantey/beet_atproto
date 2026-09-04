use beet::prelude::*;

/// One custom-domain atproto handle: the label it takes under its block's
/// domain, and the account it resolves to.
///
/// A handle always carries a did. A did is permanent and public and a handle is
/// only a name pointed at one, so "a person who has no account yet" is not a
/// state this type represents: they are simply not declared.
#[derive(
	Debug,
	Default,
	Clone,
	PartialEq,
	Eq,
	Get,
	SetWith,
	Serialize,
	Deserialize,
	Reflect,
)]
#[reflect(Default)]
pub struct AtprotoHandle {
	/// The label under the handle domain, ie the `pete` in `pete.beetmash.com`.
	name: SmolStr,
	/// The account this handle resolves to, ie `did:plc:...`.
	did: SmolStr,
}

impl AtprotoHandle {
	/// The scheme every did carries, checked rather than parsed: a typo in a
	/// declaration is caught here, and what a did METHOD means is the
	/// resolver's business, not this crate's.
	pub const DID_PREFIX: &'static str = "did:";

	/// A handle at `name` resolving to `did`.
	pub fn new(name: impl Into<SmolStr>, did: impl Into<SmolStr>) -> Self {
		Self {
			name: name.into(),
			did: did.into(),
		}
	}

	/// The handle itself, ie `pete.beetmash.com`.
	pub fn handle(&self, domain: &str) -> String {
		format!("{}.{domain}", self.name)
	}

	/// The record this handle is published at, ie `_atproto.pete.beetmash.com`.
	/// Nested under the handle's own label rather than at the apex, which is
	/// why a zone's apex-safety rules never have to make an exception for it.
	pub fn record_name(&self, domain: &str) -> String {
		format!("_atproto.{}.{domain}", self.name)
	}

	/// The record value, ie `did=did:plc:...`. The `did=` is the spec's, not
	/// decoration: a bare did in this record resolves nothing.
	pub fn record_value(&self) -> String { format!("did={}", self.did) }

	/// Reject a handle that cannot resolve: a name that is not a legal DNS
	/// label or is one of the [`RESERVED_HOSTNAMES`](DnsProvider::RESERVED_HOSTNAMES)
	/// the stack itself needs, or a did that is not one.
	pub fn validate(&self) -> Result {
		DnsProvider::validate_label(&self.name, "handle name")?;
		if DnsProvider::RESERVED_HOSTNAMES.contains(&self.name.as_str()) {
			bevybail!(
				"handle name '{}' is a reserved hostname: handles share the zone with infrastructure names",
				self.name
			);
		}
		if !self.did.starts_with(Self::DID_PREFIX) {
			bevybail!(
				"the did of handle '{}' is '{}', which is not a did: a handle resolves to an identity like `did:plc:...`, not to a handle or a url",
				self.name,
				self.did
			);
		}
		Ok(())
	}
}
