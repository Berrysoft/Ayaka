use ayaka_plugin::RawModule;
use ayaka_runtime::{plugin::*, *};
use std::collections::HashMap;

struct ModuleWrapper<'a, M: RawModule> {
    module: &'a Module<M>,
}

impl<'a, M: RawModule> ModuleWrapper<'a, M> {
    pub fn call(&self, script: &str) -> VarMap {
        let game_props = HashMap::default();
        let frontend = FrontendType::Text;
        let ctx = RawContext::default();
        let props = VarMap::from([("exec".to_string(), RawValue::Str(script.to_string()))]);
        let ctx = LineProcessContextRef {
            game_props: &game_props,
            frontend,
            ctx: &ctx,
            props: &props,
        };
        let res = self.module.dispatch_line("exec", ctx).unwrap();
        res.locals
    }
}

async fn with_ctx<M: RawModule + Send + Sync + 'static>(f: impl FnOnce(&ModuleWrapper<M>)) {
    let runtime = Runtime::load(
        "../../examples/plugins",
        env!("CARGO_MANIFEST_DIR"),
        &["ayacript", "random"],
    )
    .await
    .unwrap();
    let module = runtime.line_module("exec").unwrap();
    let wrapper = ModuleWrapper { module };
    f(&wrapper);
}

#[generic_tests::define(attrs(tokio::test))]
mod runtime_tests {
    use super::*;
    use ayaka_plugin::RawModule;
    use ayaka_plugin_wasmer::WasmerModule;
    // use ayaka_plugin_wasmi::WasmiModule;
    use ayaka_plugin_wasmtime::WasmtimeModule;

    #[tokio::test]
    async fn vars<M: RawModule + Send + Sync + 'static>() {
        with_ctx::<M>(|ctx| {
            assert_eq!(
                ctx.call(
                    "
                        a = 0;
                        a += 1;
                        a += a;
                    "
                )
                .get("a"),
                None
            );

            assert_eq!(
                ctx.call(
                    "
                        $a = 0;
                        $a += 1;
                        $a += a;
                    "
                )["a"],
                RawValue::Num(1)
            );
        })
        .await;
    }

    #[tokio::test]
    async fn if_test<M: RawModule + Send + Sync + 'static>() {
        with_ctx::<M>(|ctx| {
            assert_eq!(
                ctx.call(
                    r##"
                        $a = if(1 + 1 + 4 + 5 + 1 + 4 == 16, "sodayo", ~)
                    "##
                )["a"]
                    .get_num(),
                6
            );
            assert_eq!(
                ctx.call(
                    r##"
                        $a = if(true, "sodayo")
                    "##
                )["a"]
                    .get_str(),
                "sodayo"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn random<M: RawModule + Send + Sync + 'static>() {
        with_ctx::<M>(|ctx| {
            assert!((0..10).contains(
                &ctx.call(
                    r##"
                        $a = random.rnd(10)
                    "##
                )["a"]
                    .get_num()
            ))
        })
        .await;
    }

    // #[instantiate_tests(<WasmiModule>)]
    // mod inst_wasmi {}
    #[instantiate_tests(<WasmtimeModule>)]
    mod inst_wasmtime {}
    #[instantiate_tests(<WasmerModule>)]
    mod inst_wasmer {}
}
