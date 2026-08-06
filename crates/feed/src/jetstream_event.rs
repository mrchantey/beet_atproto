use crate::prelude::*;
use beet::prelude::*;
use serde::Deserialize;
use serde::Serialize;

/// The four public Bluesky operated Jetstream instances.
pub const PUBLIC_JETSTREAM_ENDPOINTS: [&str; 4] = [
	"wss://jetstream1.us-east.bsky.network/subscribe",
	"wss://jetstream2.us-east.bsky.network/subscribe",
	"wss://jetstream1.us-west.bsky.network/subscribe",
	"wss://jetstream2.us-west.bsky.network/subscribe",
];

/// A single Jetstream event, the JSON form of a firehose message.
///
/// Unknown kinds and collections are tolerated: `commit` is only present on
/// `kind: "commit"` events and `record` is a raw value with typed views like
/// [`Self::post_record`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JetstreamEvent {
	/// The repo (account) did this event belongs to.
	pub did: SmolStr,
	/// Event time in unix microseconds, also the resume cursor.
	pub time_us: u64,
	/// The event kind: `commit`, `identity` or `account`.
	pub kind: SmolStr,
	/// The commit payload, present when `kind` is `commit`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub commit: Option<JetstreamCommit>,
}

impl JetstreamEvent {
	/// The at uri of the committed record, when this is a commit event.
	pub fn at_uri(&self) -> Option<AtUri> {
		self.commit.as_ref().map(|commit| {
			AtUri::new(
				self.did.clone(),
				commit.collection.clone(),
				commit.rkey.clone(),
			)
		})
	}

	/// The typed post record, when this is a post commit carrying a record.
	pub fn post_record(&self) -> Option<PostRecord> {
		self.commit
			.as_ref()
			.filter(|commit| commit.collection == POST_NSID)
			.and_then(|commit| commit.record.as_ref())
			.and_then(|record| serde_json::from_value(record.clone()).ok())
	}
}

/// The repo operation payload of a commit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JetstreamCommit {
	/// The repo revision.
	#[serde(default)]
	pub rev: SmolStr,
	/// The operation: create, update or delete.
	pub operation: CommitOperation,
	/// The record collection nsid.
	pub collection: SmolStr,
	/// The record key.
	pub rkey: SmolStr,
	/// The record body, absent on deletes.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub record: Option<serde_json::Value>,
	/// The record cid, absent on deletes.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub cid: Option<SmolStr>,
}

/// A commit operation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitOperation {
	/// A record was created.
	Create,
	/// A record was updated.
	Update,
	/// A record was deleted.
	Delete,
}

#[cfg(test)]
pub(crate) mod test {
	use crate::prelude::*;
	use beet::prelude::*;

	/// Verbatim like-create event from the live stream.
	const LIKE_CREATE: &str = r#"{
		"did": "did:plc:eygmaihciaxprqvxpfvl6flk",
		"time_us": 1725911162329308,
		"kind": "commit",
		"commit": {
			"rev": "3l3qo2vutsw2b",
			"operation": "create",
			"collection": "app.bsky.feed.like",
			"rkey": "3l3qo2vuowo2b",
			"record": {
				"$type": "app.bsky.feed.like",
				"createdAt": "2024-09-09T19:46:02.102Z",
				"subject": {
					"cid": "bafyreidc6sydkkbchcyg62v77wbhzvb2mvytlmsychqgwf2xojjtirmzj4",
					"uri": "at://did:plc:wa7b35aakoll7hugkrjtf3xf/app.bsky.feed.post/3l3pte3p2e325"
				}
			},
			"cid": "bafyreidwaivazkwu67xztlmuobx35hs2lnfh3kolmgfmucldvhd3sgzcqi"
		}
	}"#;

	/// A post create in the same shape.
	pub(crate) const POST_CREATE: &str = r#"{
		"did": "did:plc:author",
		"time_us": 1725911162329308,
		"kind": "commit",
		"commit": {
			"rev": "3l3qo2vutsw2b",
			"operation": "create",
			"collection": "app.bsky.feed.post",
			"rkey": "3l3rkey",
			"record": {
				"$type": "app.bsky.feed.post",
				"createdAt": "2024-09-09T19:46:02.102Z",
				"langs": ["en"],
				"text": "have you seen alf lately?"
			},
			"cid": "bafyreipost"
		}
	}"#;

	/// A post delete: no record, no cid.
	pub(crate) const POST_DELETE: &str = r#"{
		"did": "did:plc:author",
		"time_us": 1725911163000000,
		"kind": "commit",
		"commit": {
			"rev": "3l3qo2vutsw2c",
			"operation": "delete",
			"collection": "app.bsky.feed.post",
			"rkey": "3l3rkey"
		}
	}"#;

	#[beet::test]
	fn deserializes_like_create() {
		let event = serde_json::from_str::<JetstreamEvent>(LIKE_CREATE).unwrap();
		event.did.xpect_eq("did:plc:eygmaihciaxprqvxpfvl6flk");
		event.time_us.xpect_eq(1725911162329308_u64);
		event.kind.xpect_eq("commit");
		let commit = event.commit.as_ref().unwrap();
		commit.rev.xpect_eq("3l3qo2vutsw2b");
		commit.operation.xpect_eq(CommitOperation::Create);
		commit.collection.xpect_eq("app.bsky.feed.like");
		commit.rkey.xpect_eq("3l3qo2vuowo2b");
		commit.record.as_ref().xpect_some();
		commit
			.cid
			.as_ref()
			.unwrap()
			.xpect_eq("bafyreidwaivazkwu67xztlmuobx35hs2lnfh3kolmgfmucldvhd3sgzcqi");
		// likes carry no post record
		event.post_record().xpect_none();
		event
			.at_uri()
			.unwrap()
			.to_string()
			.xpect_eq("at://did:plc:eygmaihciaxprqvxpfvl6flk/app.bsky.feed.like/3l3qo2vuowo2b");
	}

	#[beet::test]
	fn deserializes_post_create() {
		let event = serde_json::from_str::<JetstreamEvent>(POST_CREATE).unwrap();
		let post = event.post_record().unwrap();
		post.text.xpect_eq("have you seen alf lately?");
		post.created_at.xpect_eq("2024-09-09T19:46:02.102Z");
		event
			.at_uri()
			.unwrap()
			.to_string()
			.xpect_eq("at://did:plc:author/app.bsky.feed.post/3l3rkey");
	}

	#[beet::test]
	fn deserializes_post_delete() {
		let event = serde_json::from_str::<JetstreamEvent>(POST_DELETE).unwrap();
		let commit = event.commit.as_ref().unwrap();
		commit.operation.xpect_eq(CommitOperation::Delete);
		commit.record.as_ref().xpect_none();
		commit.cid.as_ref().xpect_none();
		event.post_record().xpect_none();
	}
}
