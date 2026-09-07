use crate::prelude::*;
use beet::prelude::*;

/// Every custom-domain atproto handle published under one domain, ie the
/// `beetmash.com` behind `pete.beetmash.com`.
///
/// The whole of a custom-domain handle's infrastructure is one DNS TXT record
/// per handle, `_atproto.<name>.<domain>` holding `did=did:plc:...`, or
/// `_atproto.<domain>` for the apex handle, which takes no name because it IS
/// the domain. The account itself lives on somebody's PDS and is not declared
/// here: this block publishes the record that points a name people can read at
/// the identity that already exists.
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
	/// `atproto-pete` behind `beetmash-com--atproto-pete`, or the
	/// `atproto-apex` an apex handle takes in place of the name it has none of.
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
		let mut seen = HashSet::<Option<SmolStr>>::default();
		for handle in &self.handles {
			handle.validate()?;
			if !seen.insert(handle.name().clone()) {
				match handle.name() {
					Some(name) => bevybail!(
						"handle '{name}' is declared twice on '{}': two records \
						 at one name is a handle that resolves to neither",
						self.domain
					),
					// the apex has no name to quote, and an empty '' in the
					// message above would read as a handle with a blank label
					None => bevybail!(
						"the apex handle of '{}' is declared twice: two records \
						 at one name is a handle that resolves to neither",
						self.domain
					),
				}
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
					handle.label()
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
		rendered(stack, blocks)
			.into_iter()
			.map(|(_, name, content)| (name, content))
			.collect()
	}

	/// The terraform labels every record `blocks` render to, sorted. A label is
	/// the resource's identity in state, so a wrong one is a record replaced on
	/// the next apply rather than one that never existed.
	fn labels(stack: Stack, blocks: &[AtprotoHandleBlock]) -> Vec<String> {
		rendered(stack, blocks)
			.into_iter()
			.map(|(label, _, _)| label)
			.collect()
	}

	/// The `(label, name, content)` of every record `blocks` render to under
	/// `stack`, sorted by name.
	fn rendered(
		stack: Stack,
		blocks: &[AtprotoHandleBlock],
	) -> Vec<(String, String, String)> {
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
			.iter()
			.map(|(label, record)| {
				let field = |key: &str| {
					record
						.get(key)
						.and_then(Value::as_str)
						.unwrap_or_default()
						.to_string()
				};
				(label.clone(), field("name"), field("content"))
			})
			.collect::<Vec<_>>();
		records.sort_by(|a, b| a.1.cmp(&b.1));
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

	/// The apex handle is the domain itself, so the ONE thing that changes is
	/// the name: still `_atproto.`-prefixed, but with no label between the
	/// prefix and the domain. `_atproto..example.com` (the naive format) is not
	/// a name at all, and `example.com` (dropping the prefix) would be a TXT at
	/// the apex, which is the record every zone's apex-safety rule is about.
	#[beet::test]
	fn the_apex_handle_is_the_domain() {
		let block = AtprotoHandleBlock::new("example.com")
			.with_dns(DnsProvider::cloudflare("example.com", "zone123"))
			.with_handle(AtprotoHandle::apex("did:plc:company"));
		records(Stack::new("atproto"), &[block.clone()]).xpect_eq(vec![(
			"_atproto.example.com".to_string(),
			"did=did:plc:company".to_string(),
		)]);
		// ..and the handle the probe resolves is the bare domain, not a
		// `.example.com` with an empty label in front of it
		block.handles()[0]
			.handle("example.com")
			.xpect_eq("example.com");
	}

	/// The apex and a subdomain are two independent names, so they coexist:
	/// distinct records, and distinct terraform labels. The label matters
	/// because it is the resource's identity in state, and the apex has no name
	/// to compose one from: a naive format leaves a trailing hyphen, so the
	/// sentinel is written out here.
	#[beet::test]
	fn the_apex_coexists_with_a_subdomain() {
		let block = AtprotoHandleBlock::new("example.com")
			.with_dns(DnsProvider::cloudflare("example.com", "zone123"))
			.with_handle(AtprotoHandle::apex("did:plc:company"))
			.with_handle(AtprotoHandle::new("pete", "did:plc:pete"));
		records(Stack::new("atproto"), &[block.clone()]).xpect_eq(vec![
			(
				"_atproto.example.com".to_string(),
				"did=did:plc:company".to_string(),
			),
			(
				"_atproto.pete.example.com".to_string(),
				"did=did:plc:pete".to_string(),
			),
		]);
		// pinned whole, since the sanitiser folds the declared `atproto-apex`
		// to `atproto_apex` and a trailing-hyphen label would fold to a
		// trailing underscore rather than fail
		labels(Stack::new("atproto"), &[block]).xpect_eq(vec![
			"atproto__dev__example_com_atproto_apex".to_string(),
			"atproto__dev__example_com_atproto_pete".to_string(),
		]);
	}

	/// `apex` is a real DNS label as well as the sentinel an apex handle's
	/// terraform label composes from, so declaring both is two DIFFERENT
	/// records (`_atproto.example.com` and `_atproto.apex.example.com`) at one
	/// terraform label. The config rejects that rather than letting one replace
	/// the other in state.
	#[beet::test]
	fn a_handle_named_apex_collides_with_the_apex() {
		AtprotoHandleBlock::new("example.com")
			.with_dns(DnsProvider::cloudflare("example.com", "zone123"))
			.with_handle(AtprotoHandle::apex("did:plc:company"))
			.with_handle(AtprotoHandle::new(
				AtprotoHandle::APEX_LABEL,
				"did:plc:someoneelse",
			))
			.emit(
				&Stack::new("atproto").resolve(&PackageConfig::default()),
				&Deployment::default(),
				&mut Deployment::default().create_config(
					&Stack::new("atproto").resolve(&PackageConfig::default()),
				),
			)
			.unwrap_err()
			.to_string()
			.xpect_contains("duplicate resource");
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
		// the apex has no name, so the two name checks do not apply to it, but
		// the did check and the duplicate check both still do
		handle(AtprotoHandle::apex("did:plc:company")).unwrap();
		handle(AtprotoHandle::apex("company.bsky.social"))
			.unwrap_err()
			.to_string()
			.xpect_contains("not a did");
		block()
			.with_handle(AtprotoHandle::apex("did:plc:company"))
			.with_handle(AtprotoHandle::apex("did:plc:someoneelse"))
			.validate()
			.unwrap_err()
			.to_string()
			.xpect_contains("apex handle of 'example.com' is declared twice");
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
