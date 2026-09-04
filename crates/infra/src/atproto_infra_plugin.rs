use crate::prelude::*;
use beet::prelude::*;

/// The atproto deploy types, so an entry declaring `<AtprotoHandleBlock/>` or
/// `<AtprotoHandleProbe/>` resolves them, and the block's records reach the
/// deploy render.
///
/// The counterpart of beet's own [`InfraPlugin`], which this initialises: that
/// one owns the [`DeployRender`] schedule and its sets, and every block defined
/// in beet registers into them there. A block defined out of tree registers
/// into the same schedule from its own crate, which is the whole of what an
/// external block costs.
#[derive(Default)]
pub struct AtprotoInfraPlugin;

impl Plugin for AtprotoInfraPlugin {
	fn build(&self, app: &mut App) {
		// the deploy render schedule and its two sets, which this plugin adds
		// systems to rather than declaring a second time.
		app.init_plugin::<InfraPlugin>();

		// the handle domain and the handles it publishes, spawned by tag:
		// `<AtprotoHandleBlock domain="beetmash.com"
		// handles={[{name:"pete", did:"did:plc:.."}]}/>`. Definitions, so every
		// target: a wasm consumer authors the stack it cannot apply.
		app.register_type::<AtprotoHandleBlock>()
			.register_type::<AtprotoHandle>()
			.add_systems(
				DeployRender,
				(
					declare::<AtprotoHandleBlock>
						.in_set(DeployRenderSet::Declare),
					render::<AtprotoHandleBlock>
						.in_set(DeployRenderSet::Render),
				),
			);

		// the post-apply verb, an unauthenticated http read like any other, so
		// it registers on every target beside the definitions.
		app.register_type::<AtprotoHandleProbe>()
			.register_type::<AtprotoHandleProbeAction>();
	}
}

#[cfg(test)]
mod test {
	use crate::prelude::*;
	use beet::prelude::*;
	use beet_atproto_client::prelude::*;

	/// The world an entry's markup builds into: the plugin under test plus the
	/// document machinery a `.bsx` load runs through.
	fn spawn(markup: &str) -> World {
		let mut world = (
			AsyncPlugin,
			TemplatePlugin,
			DocumentPlugin,
			AtprotoInfraPlugin,
		)
			.into_world();
		let nodes =
			BsxNode::parse_document(markup, &BsxParseConfig::bsx()).unwrap();
		world
			.spawn(())
			.insert_template(BsxTemplate::container(
				nodes,
				BsxTemplateRegistry::default(),
			))
			.unwrap();
		world.flush();
		world
	}

	/// The block authors from markup, which is the whole reason its types
	/// reflect: an entry declares the domain as a tag and the handles as a
	/// literal list.
	///
	/// A block whose type does not register resolves to NOTHING at all, so an
	/// entry declaring one would build a stack with no records in it and deploy
	/// successfully. That failure mode is the reason this test exists rather
	/// than a construction test.
	#[beet::test]
	fn the_handle_block_spawns_by_tag() {
		let mut world = spawn(
			r#"<Fragment>
				<AtprotoHandleBlock domain="example.com" dns_stage="prod"
					handles={[
						{name:"alice", did:"did:plc:alice"},
						{name:"bob", did:"did:plc:bob"},
					]}/>
				<AtprotoHandleProbe/>
			</Fragment>"#,
		);
		let block =
			world.query::<&AtprotoHandleBlock>().single(&world).unwrap();
		block.domain().as_str().xpect_eq("example.com");
		block.handles()[1].did().as_str().xpect_eq("did:plc:bob");
		block.validate().unwrap();
		// the stage guard is a field like any other, so a declaration that omits
		// it publishes and one that names another stage does not
		block
			.owns_names(
				&Stack::new("atproto")
					.with_stage("prod")
					.resolve(&PackageConfig::default()),
			)
			.xpect_true();
		// ..and the erased half every consumer of "a block, whichever kind"
		// reads is derived by the shared hook
		world
			.query::<&ErasedBlock>()
			.single(&world)
			.unwrap()
			.label
			.as_str()
			.xpect_eq("example.com");
		// the probe is a component + its action, both registered
		world
			.query::<(&AtprotoHandleProbe, &AtprotoHandleProbeAction)>()
			.single(&world)
			.unwrap()
			.0
			.appview()
			.as_str()
			.xpect_eq(PUBLIC_APPVIEW);
	}
}
