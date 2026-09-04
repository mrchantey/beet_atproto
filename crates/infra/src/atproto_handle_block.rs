use crate::prelude::*;
use beet::prelude::*;

/// Every custom-domain atproto handle published under one domain, ie the
/// `beetmash.com` behind `pete.beetmash.com`.
///
/// The whole of a custom-domain handle's infrastructure is one DNS TXT record
/// per handle, `_atproto.<name>.<domain>` holding `did=did:plc:...`. The
/// account itself lives on somebody's PDS and is not declared here: this block
/// publishes the record that points a name people can read at the identity that
/// already exists.
///
/// A zone is owned by one entry (see `ZoneAudit`), so a handle domain that is
/// also a mail domain declares this tag inside THAT stack rather than in a
/// second one: the block is its own tag precisely so it can be authored
/// wherever the zone's owner is.
#[derive(
	Debug, Clone, Get, SetWith, Serialize, Deserialize, Component, Reflect,
)]
#[reflect(Component, Default)]
#[component(immutable, on_insert = ErasedBlock::on_insert::<Self>,
	on_remove = ErasedBlock::on_remove
)]
pub struct AtprotoHandleBlock {
	/// The domain handles hang off, ie the `beetmash.com` in
	/// `_atproto.pete.beetmash.com`.
	domain: SmolStr,
	/// The handles published under it, each an [`AtprotoHandle`].
	#[set_with(skip)]
	handles: Vec<AtprotoHandle>,
	/// The zone the records are published into. Without one a block declaring
	/// handles cannot publish them at all, which is an error rather than a
	/// silent no-op: a green deploy that published no record is the one failure
	/// a handle stack cannot see.
	#[get(skip)]
	#[set_with(unwrap_option)]
	dns: Option<DnsProvider>,
	/// The stage that OWNS these names, ie the one whose deploy may publish
	/// them.
	///
	/// A stack's resources are named `<app>--<stage>--<label>`, but a handle
	/// record is not: `_atproto.pete.beetmash.com` is the real name of a real
	/// identity, and a second stage deploying the same declaration would
	/// publish a SECOND TXT record at it. Resolvers do not merge those, so the
	/// handle stops resolving deterministically and the account's name breaks
	/// for everybody.
	///
	/// So a stage that is not this one publishes nothing. Empty means no guard,
	/// which is right for a domain no other stage deploys.
	#[set_with(unwrap_option, into)]
	dns_stage: Option<SmolStr>,
}

impl Default for AtprotoHandleBlock {
	fn default() -> Self { Self::new("") }
}

impl AtprotoHandleBlock {
	/// The resource-label prefix every handle record composes under, ie the
	/// `atproto-pete` behind `beetmash-com--atproto-pete`.
	pub const RECORD_LABEL: &'static str = "atproto";

	/// A handle domain with no handles declared yet.
	pub fn new(domain: impl Into<SmolStr>) -> Self {
		Self {
			domain: domain.into(),
			handles: Vec::new(),
			dns: None,
			dns_stage: None,
		}
	}

	/// Publish one handle under this domain.
	pub fn with_handle(mut self, handle: AtprotoHandle) -> Self {
		self.handles.push(handle);
		self
	}

	/// The zone this domain's records are published into: the declared
	/// [`dns`](Self::with_dns) provider, else a Cloudflare zone read from
	/// `CLOUDFLARE_ZONE_ID`.
	///
	/// A declaration says which domain it serves and nothing about where the
	/// zone lives, exactly as it says nothing about which account it deploys
	/// into: both are properties of the launch.
	pub fn resolved_dns(&self) -> Option<DnsProvider> {
		self.dns
			.clone()
			.or_else(|| DnsProvider::cloudflare_env(self.domain.clone()))
	}

	/// Whether this stage may publish these names, ie whether it is the
	/// [`dns_stage`](Self::dns_stage) (or none was declared).
	pub fn owns_names(&self, stack: &ResolvedStack) -> bool {
		self.dns_stage
			.as_ref()
			.is_none_or(|owner| owner == stack.stage())
	}

	/// The domain with its dots as hyphens, ie `beetmash-com`. Used wherever a
	/// name must be unique per domain but cannot contain a dot, ie the
	/// terraform labels.
	pub fn slug(&self) -> String { self.domain.replace('.', "-") }

	/// Reject a declaration that cannot publish: a domain that is not a legal
	/// name, a handle that cannot resolve, or one name claimed twice.
	///
	/// Checked at config time rather than at apply time, since every one of
	/// these is a typo and the cheapest place to catch a typo is before any
	/// record exists.
	pub fn validate(&self) -> Result {
		DnsProvider::validate_label(&self.slug(), "handle domain")?;
		let mut seen = HashSet::<SmolStr>::default();
		for handle in &self.handles {
			handle.validate()?;
			if !seen.insert(handle.name().clone()) {
				bevybail!(
					"handle '{}' is declared twice on '{}': two records at one \
					 name is a handle that resolves to neither",
					handle.name(),
					self.domain
				);
			}
		}
		Ok(())
	}

	/// A terraform label for this domain's `suffix` resource, distinct from
	/// every other domain's in the same stack.
	fn label(&self, suffix: &str) -> String {
		format!("{}--{suffix}", self.slug())
	}
}

impl Block for AtprotoHandleBlock {
	/// The name this block is known by is its domain.
	fn label(&self) -> &SmolStr { &self.domain }
}

impl EmitBlock for AtprotoHandleBlock {
	/// One TXT record per handle, or none at all for a stage that does not own
	/// these names.
	fn emit(
		&self,
		stack: &ResolvedStack,
		_deployment: &Deployment,
		config: &mut terra::Config,
	) -> Result {
		self.validate()?;
		if self.handles.is_empty() {
			return Ok(());
		}
		if !self.owns_names(stack) {
			warn!(
				"stage '{}' does not own '{}', so its handle records are not published",
				stack.stage(),
				self.domain
			);
			return Ok(());
		}
		let dns = self.resolved_dns().ok_or_else(|| {
			bevyhow!(
				"handle domain '{}' publishes records but no zone resolves: \
				 set CLOUDFLARE_ZONE_ID or `with_dns` a provider",
				self.domain
			)
		})?;
		for handle in &self.handles {
			dns.emit_txt(
				stack,
				config,
				&self.label(&format!(
					"{}-{}",
					Self::RECORD_LABEL,
					handle.name()
				)),
				&handle.record_name(&self.domain),
				&handle.record_value(),
			)?;
		}
		Ok(())
	}
}

#[cfg(test)]
mod test {
	use crate::prelude::*;
	use beet::prelude::*;
	use serde_json::Value;

	/// Two handles on one domain, which is the declaration every assertion
	/// below reads. The zone is written out so nothing here depends on the
	/// environment.
	fn block() -> AtprotoHandleBlock {
		AtprotoHandleBlock::new("example.com")
			.with_dns(DnsProvider::cloudflare("example.com", "zone123"))
			.with_handle(AtprotoHandle::new("alice", "did:plc:alice"))
			.with_handle(AtprotoHandle::new("bob", "did:plc:bob"))
	}

	/// The `(name, content)` of every record `blocks` render to under `stack`,
	/// sorted. The one way a test renders, ie through the same schedule the
	/// deploy runs.
	fn records(
		stack: Stack,
		blocks: &[AtprotoHandleBlock],
	) -> Vec<(String, String)> {
		let blocks = blocks.to_vec();
		let mut world = AtprotoInfraPlugin.into_world();
		world.init_resource::<PackageConfig>();
		let root = world
			.spawn(stack)
			.with_children(|parent| {
				for block in blocks {
					parent.spawn(block);
				}
			})
			.id();
		let json = RenderScope::render(&mut world, root)
			.unwrap()
			.finish()
			.unwrap()
			.2
			.to_json()
			.into_json();
		let Some(Value::Object(records)) = json
			.get("resource")
			.and_then(|resource| resource.get("cloudflare_dns_record"))
		else {
			return Vec::new();
		};
		let mut records = records
			.values()
			.map(|record| {
				let field = |key: &str| {
					record
						.get(key)
						.and_then(Value::as_str)
						.unwrap_or_default()
						.to_string()
				};
				(field("name"), field("content"))
			})
			.collect::<Vec<_>>();
		records.sort();
		records
	}

	/// A handle IS its record, so the pair is pinned in full: a wrong name is a
	/// handle that never resolves, and a value missing the `did=` prefix is a
	/// record that exists and means nothing.
	#[beet::test]
	fn a_handle_is_one_txt_record() {
		records(Stack::new("atproto"), &[block()]).xpect_eq(vec![
			(
				"_atproto.alice.example.com".to_string(),
				"did=did:plc:alice".to_string(),
			),
			(
				"_atproto.bob.example.com".to_string(),
				"did=did:plc:bob".to_string(),
			),
		]);
	}

	/// A handle record is a real global name: two stages publishing the same
	/// declaration would put a second TXT at it and the handle would resolve to
	/// neither. Only the owning stage may publish.
	#[beet::test]
	fn only_the_owning_stage_publishes() {
		let block = block().with_dns_stage("prod");
		records(Stack::new("atproto").with_stage("prod"), &[block.clone()])
			.len()
			.xpect_eq(2);
		records(Stack::new("atproto").with_stage("drill"), &[block])
			.xpect_eq(Vec::new());
	}

	/// A typo is caught before any record exists, since the alternative is
	/// discovering it as an account whose handle the app calls invalid.
	#[beet::test]
	fn invalid_declarations_fail_at_config_time() {
		let handle = |handle: AtprotoHandle| {
			AtprotoHandleBlock::new("example.com")
				.with_dns(DnsProvider::cloudflare("example.com", "zone123"))
				.with_handle(handle)
				.validate()
		};
		handle(AtprotoHandle::new("alice", "did:plc:alice")).unwrap();
		// a name the stack itself needs
		handle(AtprotoHandle::new("www", "did:plc:alice"))
			.unwrap_err()
			.to_string()
			.xpect_contains("reserved hostname");
		// a name that is not a legal dns label at all
		handle(AtprotoHandle::new("Alice", "did:plc:alice"))
			.unwrap_err()
			.to_string()
			.xpect_contains("lowercase");
		// a handle pointed at anything other than an identity
		handle(AtprotoHandle::new("alice", "alice.bsky.social"))
			.unwrap_err()
			.to_string()
			.xpect_contains("not a did");
		// ..and one name claimed twice, which is two records at one name
		block()
			.with_handle(AtprotoHandle::new("alice", "did:plc:someoneelse"))
			.validate()
			.unwrap_err()
			.to_string()
			.xpect_contains("declared twice");
	}

	/// A domain that declares handles but resolves no zone would apply clean
	/// and publish nothing, which is a green deploy and a handle that does not
	/// exist. A domain declaring no handle at all needs no zone.
	///
	/// Native-only: the fallback zone is read from the environment, and wasm has
	/// no process environment to pin it in.
	#[cfg(not(target_arch = "wasm32"))]
	#[beet::test]
	fn handles_without_a_zone_fail() {
		// SAFETY: test-only. The empty value is what a machine with no zone
		// configured resolves, pinned so a configured one cannot pass this.
		unsafe {
			std::env::set_var("CLOUDFLARE_ZONE_ID", "");
		}
		let stack = Stack::new("atproto").resolve(&PackageConfig::default());
		let deployment = Deployment::default();
		let mut config = deployment.create_config(&stack);
		AtprotoHandleBlock::new("example.com")
			.with_handle(AtprotoHandle::new("alice", "did:plc:alice"))
			.emit(&stack, &deployment, &mut config)
			.unwrap_err()
			.to_string()
			.xpect_contains("no zone resolves");
		AtprotoHandleBlock::new("example.com")
			.emit(&stack, &deployment, &mut config)
			.unwrap();
		unsafe {
			std::env::remove_var("CLOUDFLARE_ZONE_ID");
		}
	}
}
