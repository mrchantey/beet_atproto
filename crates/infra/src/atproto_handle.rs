use beet::prelude::*;

/// One custom-domain atproto handle: the label it takes under its block's
/// domain, and the account it resolves to.
///
/// A handle always carries a did. A did is permanent and public and a handle is
/// only a name pointed at one, so "a person who has no account yet" is not a
/// state this type represents: they are simply not declared.
///
/// A handle carries a name only if it takes one. Omit it and the handle IS the
/// domain, ie `beetmash.com` rather than `pete.beetmash.com`, which is the one
/// handle a company account usually wants.
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
	/// Absent is the APEX: the handle is the bare domain, so there is no label
	/// to declare and none to validate.
	#[set_with(unwrap_option, into)]
	name: Option<SmolStr>,
	/// The account this handle resolves to, ie `did:plc:...`.
	did: SmolStr,
}

impl AtprotoHandle {
	/// The scheme every did carries, checked rather than parsed: a typo in a
	/// declaration is caught here, and what a did METHOD means is the
	/// resolver's business, not this crate's.
	pub const DID_PREFIX: &'static str = "did:";

	/// The label an apex handle's record composes under, standing in for the
	/// name it does not have: `beetmash-com--atproto-apex` rather than a
	/// trailing-hyphen `atproto-`.
	///
	/// A handle named literally `apex` beside an apex handle composes this same
	/// label, which the config's duplicate-resource check rejects at emit: two
	/// records at one label is one of them silently replacing the other in
	/// state, so it fails loudly instead.
	pub const APEX_LABEL: &'static str = "apex";

	/// A handle at `name` resolving to `did`.
	pub fn new(name: impl Into<SmolStr>, did: impl Into<SmolStr>) -> Self {
		Self {
			name: Some(name.into()),
			did: did.into(),
		}
	}

	/// The apex handle resolving to `did`: the domain itself is the handle, so
	/// it takes no label.
	pub fn apex(did: impl Into<SmolStr>) -> Self {
		Self {
			name: None,
			did: did.into(),
		}
	}

	/// The handle itself, ie `pete.beetmash.com`, or the bare `beetmash.com`
	/// for the apex.
	pub fn handle(&self, domain: &str) -> String {
		match &self.name {
			Some(name) => format!("{name}.{domain}"),
			None => domain.to_string(),
		}
	}

	/// The record this handle is published at, ie `_atproto.pete.beetmash.com`,
	/// or `_atproto.beetmash.com` for the apex.
	///
	/// Always nested under `_atproto.`, which is a name under the apex even for
	/// the apex handle, so a zone's apex-safety rules never have to make an
	/// exception for it: the apex HANDLE publishes no record AT the apex.
	pub fn record_name(&self, domain: &str) -> String {
		match &self.name {
			Some(name) => format!("_atproto.{name}.{domain}"),
			None => format!("_atproto.{domain}"),
		}
	}

	/// The label fragment this handle's record composes under, ie the `pete` in
	/// `beetmash-com--atproto-pete`, or [`APEX_LABEL`](Self::APEX_LABEL).
	pub fn label(&self) -> &str {
		self.name.as_deref().unwrap_or(Self::APEX_LABEL)
	}

	/// The record value, ie `did=did:plc:...`. The `did=` is the spec's, not
	/// decoration: a bare did in this record resolves nothing.
	pub fn record_value(&self) -> String { format!("did={}", self.did) }

	/// Reject a handle that cannot resolve: a name that is not a legal DNS
	/// label or is one of the [`RESERVED_HOSTNAMES`](DnsProvider::RESERVED_HOSTNAMES)
	/// the stack itself needs, or a did that is not one.
	///
	/// An apex handle has no name, so neither name check applies to it: both
	/// constrain a label taken under the domain, and the apex takes none.
	pub fn validate(&self) -> Result {
		if let Some(name) = &self.name {
			DnsProvider::validate_label(name, "handle name")?;
			if DnsProvider::RESERVED_HOSTNAMES.contains(&name.as_str()) {
				bevybail!(
					"handle name '{name}' is a reserved hostname: handles share the zone with infrastructure names"
				);
			}
		}
		if !self.did.starts_with(Self::DID_PREFIX) {
			bevybail!(
				"the did of the {} handle is '{}', which is not a did: a handle resolves to an identity like `did:plc:...`, not to a handle or a url",
				match &self.name {
					Some(name) => format!("'{name}'"),
					None => "apex".to_string(),
				},
				self.did
			);
		}
		Ok(())
	}
}
