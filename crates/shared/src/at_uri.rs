use beet::prelude::*;

/// The nsid collection for feed generator declaration records.
pub const FEED_GENERATOR_NSID: &str = "app.bsky.feed.generator";
/// The nsid collection for posts.
pub const POST_NSID: &str = "app.bsky.feed.post";

/// An `at://` uri in the `authority/collection/rkey` record form, eg
/// `at://did:plc:abc/app.bsky.feed.generator/whats-alf`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AtUri {
	/// The repo holding the record, typically a did.
	pub authority: SmolStr,
	/// The record collection nsid, eg `app.bsky.feed.post`.
	pub collection: SmolStr,
	/// The record key within the collection.
	pub rkey: SmolStr,
}

impl AtUri {
	/// An at uri from its three parts.
	pub fn new(
		authority: impl Into<SmolStr>,
		collection: impl Into<SmolStr>,
		rkey: impl Into<SmolStr>,
	) -> Self {
		Self {
			authority: authority.into(),
			collection: collection.into(),
			rkey: rkey.into(),
		}
	}

	/// A feed generator declaration uri:
	/// `at://{did}/app.bsky.feed.generator/{rkey}`.
	pub fn feed_generator(
		did: impl Into<SmolStr>,
		rkey: impl Into<SmolStr>,
	) -> Self {
		Self::new(did, FEED_GENERATOR_NSID, rkey)
	}

	/// A post uri: `at://{did}/app.bsky.feed.post/{rkey}`.
	pub fn post(did: impl Into<SmolStr>, rkey: impl Into<SmolStr>) -> Self {
		Self::new(did, POST_NSID, rkey)
	}

	/// Strict parse of the `at://authority/collection/rkey` record form.
	pub fn parse(uri: &str) -> Result<Self> {
		let rest = uri
			.strip_prefix("at://")
			.ok_or_else(|| bevyhow!("expected an at:// scheme: {uri}"))?;
		let mut parts = rest.split('/');
		match (parts.next(), parts.next(), parts.next(), parts.next()) {
			(Some(authority), Some(collection), Some(rkey), None)
				if !authority.is_empty()
					&& !collection.is_empty()
					&& !rkey.is_empty() =>
			{
				Self::new(authority, collection, rkey).xok()
			}
			_ => bevybail!("expected at://authority/collection/rkey: {uri}"),
		}
	}
}

impl core::fmt::Display for AtUri {
	fn fmt(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(
			formatter,
			"at://{}/{}/{}",
			self.authority, self.collection, self.rkey
		)
	}
}

impl core::str::FromStr for AtUri {
	type Err = BevyError;
	fn from_str(uri: &str) -> Result<Self> { Self::parse(uri) }
}

#[cfg(test)]
mod test {
	use crate::prelude::*;
	use beet::prelude::*;

	#[beet::test]
	fn parses() {
		let uri = AtUri::parse(
			"at://did:plc:abc/app.bsky.feed.generator/whats-alf",
		)
		.unwrap();
		uri.authority.xpect_eq("did:plc:abc");
		uri.collection.xpect_eq(FEED_GENERATOR_NSID);
		uri.rkey.xpect_eq("whats-alf");
	}

	#[beet::test]
	fn roundtrips() {
		AtUri::feed_generator("did:plc:abc", "whats-alf")
			.to_string()
			.xmap(|uri| AtUri::parse(&uri).unwrap().to_string())
			.xpect_eq("at://did:plc:abc/app.bsky.feed.generator/whats-alf");
	}

	#[beet::test]
	fn rejects_malformed() {
		AtUri::parse("did:plc:abc/coll/rkey").xpect_err();
		AtUri::parse("at://did:plc:abc/coll").xpect_err();
		AtUri::parse("at://did:plc:abc/coll/rkey/extra").xpect_err();
		AtUri::parse("at://did:plc:abc//rkey").xpect_err();
		AtUri::parse("at:///coll/rkey").xpect_err();
	}
}
