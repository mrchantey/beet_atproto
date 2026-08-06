use serde::Deserialize;
use serde::Serialize;

/// The fields of an `app.bsky.feed.post` record these crates read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostRecord {
	/// The post text.
	#[serde(default)]
	pub text: String,
	/// The author supplied creation time.
	#[serde(default, rename = "createdAt")]
	pub created_at: String,
}
