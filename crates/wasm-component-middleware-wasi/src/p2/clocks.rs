use wasm_component_middleware::MiddlewareView;
use wasmtime::component::{Linker, Resource};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::clocks::WasiClocksView;
use wasmtime_wasi::p2::DynPollable;
use wasmtime_wasi::p2::bindings::clocks::{monotonic_clock, wall_clock};

use super::WASI_VERSION;
use super::gate::{Gate, GateData, gate, project};

impl<T> wall_clock::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn now(&mut self) -> wasmtime::Result<wall_clock::Datetime> {
        gate!(trap self, WASI_VERSION, "wasi:clocks/wall-clock", "now", handles = [], args = (), delegate = |state: &mut T| wall_clock::Host::now(&mut state.clocks()))
    }

    fn resolution(&mut self) -> wasmtime::Result<wall_clock::Datetime> {
        gate!(trap self, WASI_VERSION, "wasi:clocks/wall-clock", "resolution", handles = [], args = (), delegate = |state: &mut T| wall_clock::Host::resolution(&mut state.clocks()))
    }
}

impl<T> monotonic_clock::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn now(&mut self) -> wasmtime::Result<monotonic_clock::Instant> {
        gate!(trap self, WASI_VERSION, "wasi:clocks/monotonic-clock", "now", handles = [], args = (), delegate = |state: &mut T| monotonic_clock::Host::now(&mut state.clocks()))
    }

    fn resolution(&mut self) -> wasmtime::Result<monotonic_clock::Instant> {
        gate!(trap self, WASI_VERSION, "wasi:clocks/monotonic-clock", "resolution", handles = [], args = (), delegate = |state: &mut T| monotonic_clock::Host::resolution(&mut state.clocks()))
    }

    fn subscribe_instant(
        &mut self,
        when: monotonic_clock::Instant,
    ) -> wasmtime::Result<Resource<DynPollable>> {
        gate!(trap self, WASI_VERSION, "wasi:clocks/monotonic-clock", "subscribe-instant", handles = [], args = [when = when], delegate = |state: &mut T| monotonic_clock::Host::subscribe_instant(&mut state.clocks(), when), produced = |value: &Resource<DynPollable>| vec![value.rep()])
    }

    fn subscribe_duration(
        &mut self,
        duration: monotonic_clock::Duration,
    ) -> wasmtime::Result<Resource<DynPollable>> {
        gate!(trap self, WASI_VERSION, "wasi:clocks/monotonic-clock", "subscribe-duration", handles = [], args = [duration = duration], delegate = |state: &mut T| monotonic_clock::Host::subscribe_duration(&mut state.clocks(), duration), produced = |value: &Resource<DynPollable>| vec![value.rep()])
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    wall_clock::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    monotonic_clock::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}

#[cfg(test)]
mod tests {
    use wasm_component_middleware::{
        Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx, Outcome,
    };
    use wasmtime::component::ResourceTable;
    use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView};

    use super::*;

    struct State {
        middleware: Option<MiddlewareCtx<Self>>,
        table: ResourceTable,
        wasi: WasiCtx,
        calls: usize,
    }

    impl MiddlewareView for State {
        fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
            self.middleware.as_mut().unwrap()
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

    struct Count;

    impl Layer<State> for Count {
        type Frame = ();

        fn before(&self, state: &mut State, _call: &Call<'_>) -> Result<(), Denied> {
            state.calls += 1;
            Ok(())
        }

        fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
    }

    #[test]
    fn wall_clock_dispatches_through_the_chain() {
        let chain = Chain::builder().layer(Count).build();
        let mut state = State {
            middleware: Some(MiddlewareCtx::new(chain, InvocationContext::new("clock"))),
            table: ResourceTable::new(),
            wasi: WasiCtxBuilder::new().build(),
            calls: 0,
        };

        wall_clock::Host::now(&mut project(&mut state)).unwrap();

        assert_eq!(state.calls, 1);
    }
}
