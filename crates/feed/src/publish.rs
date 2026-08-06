use crate::prelude::*;
use beet::prelude::*;
use serde::Deserialize;

/// A PDS session, the `createSession` response fields we use.
#[derive(Debug, Clone, Deserialize)]
struct Session {
	did: String,
	#[serde(rename = "accessJwt")]
	access_jwt: String,
}

/// Publishes the feed generator declaration record to the user's PDS, the
/// `scripts/publishFeedGen.ts` equivalent. This is what makes the feed appear
/// in the Bluesky app, addressed as
/// `at://{publisher}/app.bsky.feed.generator/{record_name}`.
///
/// Runs `com.atproto.server.createSession` then `com.atproto.repo.putRecord`
/// over plain xrpc, so the consumer must enable an http transport (eg the
/// `ureq` and a tls feature). Re-running updates the display data in place.
#[derive(Debug, Clone)]
pub struct PublishFeed {
	/// The PDS service url, defaults to `https://bsky.social`.
	pub service: String,
	/// The account handle, eg `alice.bsky.social`.
	pub handle: String,
	/// An app password from the Bluesky app settings, never the account
	/// password.
	pub password: String,
	/// The record key, the tail of the feed's url and at uri.
	pub record_name: SmolStr,
	/// The display name shown in the app.
	pub display_name: String,
	/// An optional description shown under the feed.
	pub description: Option<String>,
	/// The feed generator service did, eg `did:web:feed.example.com`.
	pub feed_did: SmolStr,
}

impl PublishFeed {
	/// A publish request against the default `https://bsky.social` PDS.
	pub fn new(
		feed_did: impl Into<SmolStr>,
		handle: impl Into<String>,
		password: impl Into<String>,
		record_name: impl Into<SmolStr>,
		display_name: impl Into<String>,
	) -> Self {
		Self {
			service: "https://bsky.social".into(),
			handle: handle.into(),
			password: password.into(),
			record_name: record_name.into(),
			display_name: display_name.into(),
			description: None,
			feed_did: feed_did.into(),
		}
	}

	/// Add a description shown under the feed.
	pub fn with_description(mut self, description: impl Into<String>) -> Self {
		self.description = Some(description.into());
		self
	}

	/// Use a custom PDS service url.
	pub fn with_service(mut self, service: impl Into<String>) -> Self {
		self.service = service.into();
		self
	}

	/// Publish the declaration record, returning its at uri.
	pub async fn publish(self) -> Result<AtUri> {
		let session = self.create_session().await?;
		let record =
			self.record_json(&time_ext::format_iso8601(time_ext::now()));
		Request::post(format!(
			"{}/xrpc/com.atproto.repo.putRecord",
			self.service
		))
		.with_auth_bearer(&session.access_jwt)
		.with_json_body(&serde_json::json!({
			"repo": session.did,
			"collection": FEED_GENERATOR_NSID,
			"rkey": self.record_name,
			"record": record,
		}))?
		.send()
		.await?
		.into_result()
		.await?;
		AtUri::feed_generator(session.did, self.record_name.clone()).xok()
	}

	/// Delete the declaration record, removing the feed from the app.
	pub async fn unpublish(self) -> Result<()> {
		let session = self.create_session().await?;
		Request::post(format!(
			"{}/xrpc/com.atproto.repo.deleteRecord",
			self.service
		))
		.with_auth_bearer(&session.access_jwt)
		.with_json_body(&serde_json::json!({
			"repo": session.did,
			"collection": FEED_GENERATOR_NSID,
			"rkey": self.record_name,
		}))?
		.send()
		.await?
		.into_result()
		.await?;
		Ok(())
	}

	async fn create_session(&self) -> Result<Session> {
		Request::post(format!(
			"{}/xrpc/com.atproto.server.createSession",
			self.service
		))
		.with_json_body(&serde_json::json!({
			"identifier": self.handle,
			"password": self.password,
		}))?
		.send()
		.await?
		.into_result()
		.await?
		.json::<Session>()
		.await
	}

	/// The `app.bsky.feed.generator` record body.
	fn record_json(&self, created_at: &str) -> serde_json::Value {
		let mut record = serde_json::json!({
			"$type": FEED_GENERATOR_NSID,
			"did": self.feed_did,
			"displayName": self.display_name,
			"createdAt": created_at,
		});
		if let Some(description) = &self.description {
			record["description"] =
				serde_json::Value::String(description.clone());
		}
		record
	}
}

#[cfg(test)]
mod test {
	use crate::prelude::*;
	use beet::prelude::*;

	#[beet::test]
	fn builds_the_record() {
		let record = PublishFeed::new(
			"did:web:feed.example.com",
			"alice.bsky.social",
			"app-password",
			"whats-alf",
			"What's Alf",
		)
		.with_description("posts about alf")
		.record_json("2024-09-09T19:46:02.102Z");
		record["$type"].as_str().unwrap().xpect_eq(FEED_GENERATOR_NSID);
		record["did"]
			.as_str()
			.unwrap()
			.xpect_eq("did:web:feed.example.com");
		record["displayName"].as_str().unwrap().xpect_eq("What's Alf");
		record["description"]
			.as_str()
			.unwrap()
			.xpect_eq("posts about alf");
		record["createdAt"]
			.as_str()
			.unwrap()
			.xpect_eq("2024-09-09T19:46:02.102Z");
	}

	#[beet::test]
	fn omits_empty_description() {
		PublishFeed::new("did:web:x", "a", "b", "c", "d")
			.record_json("now")
			.get("description")
			.xpect_none();
	}
}
