#![allow(missing_docs)]

use std::collections::BTreeSet;
use std::fs;

use wasm_component_middleware::{
    Call, Chain, Denied, InvocationContext, Layer, Logger, MiddlewareCtx, MiddlewareView, Outcome,
};
use wasm_component_middleware_wasi::OpenFiles;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "guest/wit",
    world: "sandbox",
});

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

const PREOPEN_NAMES: [&str; 2] = ["public", "private"];
const PATH_CALLS: [&str; 10] = [
    "[method]descriptor.create-directory-at",
    "[method]descriptor.stat-at",
    "[method]descriptor.set-times-at",
    "[method]descriptor.link-at",
    "[method]descriptor.open-at",
    "[method]descriptor.readlink-at",
    "[method]descriptor.remove-directory-at",
    "[method]descriptor.rename-at",
    "[method]descriptor.symlink-at",
    "[method]descriptor.unlink-file-at",
];

#[derive(Default)]
struct PrivateDescriptors(BTreeSet<u32>);

enum Track {
    Nothing,
    Preopens,
    PrivateOpen,
}

struct RefusePrivate;

impl Layer<State> for RefusePrivate {
    type Frame = Track;

    fn before(&self, state: &mut State, call: &Call<'_>) -> Result<Track, Denied> {
        let context = state.middleware().context_mut();
        if context.get::<PrivateDescriptors>().is_none() {
            context.insert(PrivateDescriptors::default());
        }
        let private = context
            .get_mut::<PrivateDescriptors>()
            .ok_or_else(|| Denied::new("descriptor policy state is unavailable"))?;

        if call.interface == Some("wasi:filesystem/types")
            && call.function == "[resource-drop]descriptor"
        {
            for handle in call.handles {
                private.0.remove(handle);
            }
            return Ok(Track::Nothing);
        }

        let uses_private = call.handles.iter().any(|handle| private.0.contains(handle));
        if call.interface == Some("wasi:filesystem/types")
            && PATH_CALLS.contains(&call.function)
            && uses_private
        {
            return Err(Denied::new("descriptor is private"));
        }

        if call.interface == Some("wasi:filesystem/preopens") && call.function == "get-directories"
        {
            Ok(Track::Preopens)
        } else if call.interface == Some("wasi:filesystem/types")
            && call.function == "[method]descriptor.open-at"
            && uses_private
        {
            Ok(Track::PrivateOpen)
        } else {
            Ok(Track::Nothing)
        }
    }

    fn after(&self, state: &mut State, _call: &Call<'_>, track: Track, outcome: Outcome<'_>) {
        let Outcome::Returned(completion) = outcome else {
            return;
        };
        let Some(private) = state
            .middleware()
            .context_mut()
            .get_mut::<PrivateDescriptors>()
        else {
            return;
        };
        match track {
            Track::Preopens => {
                private.0.extend(
                    PREOPEN_NAMES
                        .iter()
                        .zip(&completion.produced)
                        .filter_map(|(name, rep)| (*name == "private").then_some(*rep)),
                );
            }
            Track::PrivateOpen => private.0.extend(&completion.produced),
            Track::Nothing => {}
        }
    }
}

fn main() -> wasmtime::Result<()> {
    let directory = tempfile::tempdir()?;
    let public = directory.path().join("public");
    let private = directory.path().join("private");
    fs::create_dir(&public)?;
    fs::create_dir(&private)?;
    for (name, contents) in [
        ("note.txt", "hello"),
        ("one.txt", "one"),
        ("two.txt", "two"),
        ("three.txt", "three"),
    ] {
        fs::write(public.join(name), contents)?;
    }
    fs::write(private.join("secret.txt"), "classified")?;

    let engine = Engine::default();
    let component = Component::from_file(&engine, guest_build::example("sandbox"))?;
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker)?;

    let stdout = MemoryOutputPipe::new(4096);
    let mut wasi = WasiCtxBuilder::new();
    wasi.stdout(stdout.clone())
        .preopened_dir(&public, PREOPEN_NAMES[0], FsPerms::ReadWrite)?
        .preopened_dir(&private, PREOPEN_NAMES[1], FsPerms::ReadWrite)?;
    let chain = Chain::builder()
        .layer(Logger::stderr())
        .layer(OpenFiles::new(4))
        .layer(RefusePrivate)
        .build();
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("sandbox")),
            table: ResourceTable::new(),
            wasi: wasi.build(),
        },
    );

    let guest = Sandbox::instantiate(&mut store, &component, &linker)?;
    guest.call_run(&mut store)?;
    print!("{}", String::from_utf8(stdout.contents().to_vec())?);
    Ok(())
}
