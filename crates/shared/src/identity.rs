use serde::Deserialize;
use serde::Serialize;

/// The `com.atproto.identity.resolveHandle` output: the did a handle points at
/// right now.
///
/// The one direction that has to be re-asked. A did is permanent and a handle
/// is a name pointed at one, so a handle may move to another account (or stop
/// resolving entirely when its dns record goes), and only the resolver knows
/// where it points today.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveHandleResponse {
	/// The account did, eg `did:plc:z72i7hdynmk6r22z27h6tvur`.
	pub did: String,
}
