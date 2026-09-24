#![allow(missing_docs)]

use std::path::PathBuf;

use wasm_component_middleware::{Chain, InvocationContext, Logger, MiddlewareCtx, MiddlewareView};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

struct State {
    middleware: MiddlewareCtx<Self>,
    table: ResourceTable,
    wasi: WasiCtx,
}

impl MiddlewareView for State {
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
        &mut self.middleware
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

struct Options {
    component: PathBuf,
    guest_args: Vec<String>,
    inherit_env: bool,
}

impl Options {
    fn parse() -> wasmtime::Result<Self> {
        let mut args = std::env::args().skip(1);
        let first = args.next().ok_or_else(|| {
            wasmtime::Error::msg("usage: run [--inherit-env] COMPONENT [ARGS...]")
        })?;
        let (inherit_env, component) = if first == "--inherit-env" {
            let component = args.next().ok_or_else(|| {
                wasmtime::Error::msg("usage: run [--inherit-env] COMPONENT [ARGS...]")
            })?;
            (true, component)
        } else {
            (false, first)
        };
        let mut guest_args = vec![component.clone()];
        guest_args.extend(args);
        Ok(Self {
            component: component.into(),
            guest_args,
            inherit_env,
        })
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        if let Some(exit) = error.downcast_ref::<wasmtime_wasi::I32Exit>() {
            std::process::exit(exit.0);
        }
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> wasmtime::Result<()> {
    let options = Options::parse()?;
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, &options.component)?;
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker)?;
    wasm_component_middleware_wasi::p3::add_to_linker(&mut linker)?;
    wasm_component_middleware_wasi::verify_routing(&engine, &component, [])?;

    let mut wasi = WasiCtxBuilder::new();
    wasi.inherit_stdio()
        .args(&options.guest_args)
        .preopened_dir(std::env::current_dir()?, ".", FsPerms::ReadOnly)?;
    if options.inherit_env {
        wasi.inherit_env();
    }
    let chain = Chain::builder().layer(Logger::stderr()).build();
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("run")),
            table: ResourceTable::new(),
            wasi: wasi.build(),
        },
    );

    if is_preview_3_command(&component, &engine) {
        run_p3(&mut store, &component, &linker).await
    } else {
        run_p2(&mut store, &component, &linker).await
    }
}

fn is_preview_3_command(component: &Component, engine: &Engine) -> bool {
    component
        .component_type()
        .exports(engine)
        .any(|(name, _)| name == "wasi:cli/run@0.3.0")
}

async fn run_p2(
    store: &mut Store<State>,
    component: &Component,
    linker: &Linker<State>,
) -> wasmtime::Result<()> {
    let command =
        wasmtime_wasi::p2::bindings::Command::instantiate_async(&mut *store, component, linker)
            .await?;
    command
        .wasi_cli_run()
        .call_run(store)
        .await?
        .map_err(|()| wasmtime::Error::msg("component returned failure"))
}

async fn run_p3(
    store: &mut Store<State>,
    component: &Component,
    linker: &Linker<State>,
) -> wasmtime::Result<()> {
    let command =
        wasmtime_wasi::p3::bindings::Command::instantiate_async(&mut *store, component, linker)
            .await?;
    store
        .run_concurrent(async move |accessor| command.wasi_cli_run().call_run(accessor).await)
        .await??
        .map_err(|()| wasmtime::Error::msg("component returned failure"))
}
