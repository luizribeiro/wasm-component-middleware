//! Build-time access to WebAssembly components used by workspace tests.
//!
//! The components are built in an isolated guest workspace so host workspace
//! commands never compile guest crates for the native target.

use std::path::Path;

/// Returns the path to the guest that imports host identity and logging.
#[must_use]
pub fn hello() -> &'static Path {
    Path::new(env!("HELLO_COMPONENT"))
}

/// Returns the path to the synchronous Preview 2 smoke component.
#[must_use]
pub fn smoke_p2() -> &'static Path {
    Path::new(env!("SMOKE_P2_COMPONENT"))
}

/// Returns the path to the asynchronous Preview 3 smoke component.
#[must_use]
pub fn smoke_p3() -> &'static Path {
    Path::new(env!("SMOKE_P3_COMPONENT"))
}

#[cfg(test)]
mod tests {
    use wasmtime::component::{Component, Linker, ResourceTable};
    use wasmtime::{Config, Engine, Result, Store};
    use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

    mod p2 {
        wasmtime::component::bindgen!({
            path: "../../guests/smoke-p2/wit",
            world: "smoke-p2",
            exports: { default: async },
            require_store_data_send: true,
        });
    }

    mod p3 {
        wasmtime::component::bindgen!({
            path: "../../guests/smoke-p3/wit",
            world: "smoke-p3",
            exports: { default: async | store },
            require_store_data_send: true,
        });
    }

    struct State {
        table: ResourceTable,
        wasi: WasiCtx,
    }

    impl State {
        fn new() -> Self {
            Self {
                table: ResourceTable::new(),
                wasi: WasiCtxBuilder::new().build(),
            }
        }
    }

    impl WasiView for State {
        fn ctx(&mut self) -> WasiCtxView<'_> {
            WasiCtxView {
                ctx: &mut self.wasi,
                table: &mut self.table,
            }
        }
    }

    fn engine(concurrent: bool) -> Result<Engine> {
        let mut config = Config::new();
        if concurrent {
            config.wasm_component_model_async(true);
            config.concurrency_support(true);
        }
        Engine::new(&config)
    }

    fn linker(engine: &Engine) -> Result<Linker<State>> {
        let mut linker = Linker::new(engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        Ok(linker)
    }

    #[tokio::test]
    async fn instantiates_p2_guest_and_greets() -> Result<()> {
        let engine = engine(false)?;
        let linker = linker(&engine)?;
        let component = Component::from_file(&engine, super::smoke_p2())?;
        let mut store = Store::new(&engine, State::new());
        let guest = p2::SmokeP2::instantiate_async(&mut store, &component, &linker).await?;

        let greeting = guest
            .test_smoke_p2_greeter()
            .call_greet(&mut store, "Ada")
            .await?;

        assert_eq!(greeting, "Hello, Ada!");
        Ok(())
    }

    #[tokio::test]
    async fn instantiates_p3_guest_and_greets_concurrently() -> Result<()> {
        let engine = engine(true)?;
        let linker = linker(&engine)?;
        let component = Component::from_file(&engine, super::smoke_p3())?;
        let mut store = Store::new(&engine, State::new());
        let guest = p3::SmokeP3::instantiate_async(&mut store, &component, &linker).await?;

        let greeting = store
            .run_concurrent(async move |accessor| {
                guest
                    .test_smoke_p3_greeter()
                    .call_greet(accessor, "Grace".to_owned())
                    .await
            })
            .await??;

        assert_eq!(greeting, "Hello, Grace!");
        Ok(())
    }
}
