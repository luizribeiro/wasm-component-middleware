use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};

use wasm_component_middleware::{
    ArgumentValue, Arguments, Call, Completion, Direction, MiddlewareView,
};
use wasmtime::AsContextMut;
use wasmtime::StoreContextMut;
use wasmtime::component::{
    Access, ComponentType, Destination, FutureConsumer, FutureProducer, FutureReader, Lift, Lower,
    Source, StreamConsumer, StreamProducer, StreamReader, StreamResult, VecBuffer,
};

use crate::gate::{GateData, Unrelayed};

pub(crate) struct Relayed<const CAPACITY: usize>;

pub(crate) trait RelayMode: Send + Sync + 'static {
    const CAPACITY: Option<usize>;
}

impl RelayMode for Unrelayed {
    const CAPACITY: Option<usize> = None;
}

impl<const CAPACITY: usize> RelayMode for Relayed<CAPACITY> {
    const CAPACITY: Option<usize> = Some(CAPACITY);
}

#[derive(Clone)]
pub(crate) struct Origin {
    pub(crate) call_id: u64,
    pub(crate) interface: &'static str,
    pub(crate) version: &'static str,
    pub(crate) function: &'static str,
    pub(crate) handles: Arc<[u32]>,
}

pub(crate) struct Shared<C> {
    bytes: VecDeque<u8>,
    in_flight: usize,
    bound: usize,
    input_closed: bool,
    output_closed: bool,
    completion: Option<C>,
    consumer_waker: Option<Waker>,
    producer_waker: Option<Waker>,
    future_waker: Option<Waker>,
    denial: Option<C>,
    denied: bool,
}

type RelayState<C> = Arc<Mutex<Shared<C>>>;

impl<C> Shared<C> {
    fn lock(shared: &Arc<Mutex<Self>>) -> MutexGuard<'_, Self> {
        shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn wake_all(&mut self) {
        for waker in [
            self.consumer_waker.take(),
            self.producer_waker.take(),
            self.future_waker.take(),
        ]
        .into_iter()
        .flatten()
        {
            waker.wake();
        }
    }

    fn deny(&mut self) {
        self.input_closed = true;
        self.denied = true;
        self.finish_denial_if_drained();
        self.wake_all();
    }

    fn finish_denial_if_drained(&mut self) {
        if self.denied && self.bytes.is_empty() && self.in_flight == 0 && self.completion.is_none()
        {
            self.completion = self.denial.take();
            if let Some(waker) = self.future_waker.take() {
                waker.wake();
            }
        }
    }
}

struct Consumer<C> {
    shared: RelayState<C>,
    origin: Origin,
}

impl<C> Drop for Consumer<C> {
    fn drop(&mut self) {
        let mut shared = Shared::lock(&self.shared);
        shared.input_closed = true;
        shared.wake_all();
    }
}

impl<T, C> StreamConsumer<T> for Consumer<C>
where
    T: MiddlewareView + 'static,
    C: Send + 'static,
{
    type Item = u8;

    fn poll_consume(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut store: StoreContextMut<T>,
        mut source: Source<'_, u8>,
        finish: bool,
    ) -> Poll<wasmtime::Result<StreamResult>> {
        let remaining = source.remaining(&mut store);
        if remaining == 0 {
            return Poll::Ready(Ok(if finish {
                StreamResult::Cancelled
            } else {
                StreamResult::Completed
            }));
        }

        let (available, buffered) = {
            let mut shared = Shared::lock(&self.shared);
            if shared.output_closed {
                return Poll::Ready(Ok(StreamResult::Dropped));
            }
            let buffered = shared.bytes.len().saturating_add(shared.in_flight);
            let available = shared.bound.saturating_sub(buffered);
            if available == 0 {
                if finish {
                    return Poll::Ready(Ok(StreamResult::Cancelled));
                }
                shared.consumer_waker = Some(cx.waker().clone());
                return Poll::Pending;
            }
            (available, buffered)
        };
        let count = remaining.min(available);
        let chunk = {
            let mut direct = source.reborrow().as_direct(store.as_context_mut());
            direct.remaining()[..count].to_vec()
        };
        let arguments = Arguments::new()
            .with("bytes", ArgumentValue::owned_bytes(chunk.clone()))
            .with(
                "buffered",
                u64::try_from(buffered + count).unwrap_or(u64::MAX),
            );
        let call = Call::new(self.origin.call_id, Direction::Import, self.origin.function)
            .in_interface(self.origin.interface, Some(self.origin.version))
            .with_handles(&self.origin.handles)
            .with_args(&arguments);
        let chain = Arc::clone(store.data_mut().middleware().chain());
        if chain
            .dispatch(store.data_mut(), &call, |_| Ok(((), Completion::default())))
            .is_err()
        {
            Shared::lock(&self.shared).deny();
            return Poll::Ready(Ok(StreamResult::Dropped));
        }

        source.as_direct(store.as_context_mut()).mark_read(count);

        let mut shared = Shared::lock(&self.shared);
        shared.bytes.extend(chunk);
        if let Some(waker) = shared.producer_waker.take() {
            waker.wake();
        }
        if shared.bytes.len() == shared.bound && count < remaining {
            shared.consumer_waker = Some(cx.waker().clone());
            Poll::Pending
        } else {
            Poll::Ready(Ok(StreamResult::Completed))
        }
    }
}

struct Producer<C> {
    shared: RelayState<C>,
}

impl<C> Drop for Producer<C> {
    fn drop(&mut self) {
        let mut shared = Shared::lock(&self.shared);
        shared.output_closed = true;
        shared.wake_all();
    }
}

impl<T, C> StreamProducer<T> for Producer<C>
where
    C: Send + 'static,
{
    type Item = u8;
    type Buffer = VecBuffer<u8>;

    fn poll_produce<'a>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut store: StoreContextMut<'a, T>,
        mut destination: Destination<'a, u8, Self::Buffer>,
        finish: bool,
    ) -> Poll<wasmtime::Result<StreamResult>> {
        let mut shared = Shared::lock(&self.shared);
        let capacity = destination.remaining(&mut store).unwrap_or(shared.bound);
        if shared.in_flight != 0 {
            shared.in_flight = 0;
            if let Some(waker) = shared.consumer_waker.take() {
                waker.wake();
            }
        }
        if capacity == 0 && !shared.bytes.is_empty() {
            return Poll::Ready(Ok(StreamResult::Completed));
        }
        if !shared.bytes.is_empty() {
            let count = capacity.min(shared.bytes.len());
            destination.set_buffer(shared.bytes.drain(..count).collect::<Vec<_>>().into());
            shared.in_flight = count;
            return Poll::Ready(Ok(StreamResult::Completed));
        }
        if shared.input_closed {
            shared.finish_denial_if_drained();
            return Poll::Ready(Ok(StreamResult::Dropped));
        }
        if finish {
            return Poll::Ready(Ok(StreamResult::Cancelled));
        }
        shared.producer_waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

struct CompletionConsumer<C> {
    shared: RelayState<C>,
}

impl<T, C> FutureConsumer<T> for CompletionConsumer<C>
where
    C: Lift + Send + Sync + 'static,
{
    type Item = C;

    fn poll_consume(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        mut store: StoreContextMut<T>,
        mut source: Source<'_, C>,
        finish: bool,
    ) -> Poll<wasmtime::Result<()>> {
        let mut item = None;
        source.read(&mut store, &mut item)?;
        if let Some(item) = item {
            let mut shared = Shared::lock(&self.shared);
            if !shared.denied && shared.completion.is_none() {
                shared.completion = Some(item);
            }
            shared.input_closed = true;
            shared.finish_denial_if_drained();
            shared.wake_all();
            Poll::Ready(Ok(()))
        } else if finish {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}

struct CompletionProducer<C> {
    shared: RelayState<C>,
}

impl<T, C> FutureProducer<T> for CompletionProducer<C>
where
    C: Send + 'static,
{
    type Item = C;

    fn poll_produce(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        _store: StoreContextMut<T>,
        finish: bool,
    ) -> Poll<wasmtime::Result<Option<C>>> {
        let mut shared = Shared::lock(&self.shared);
        if let Some(result) = shared.completion.take() {
            Poll::Ready(Ok(Some(result)))
        } else if finish {
            Poll::Ready(Ok(None))
        } else {
            shared.future_waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub(crate) fn relay_bytes<T, M, C>(
    store: &mut Access<'_, T, GateData<T, M>>,
    input: StreamReader<u8>,
    origin: Origin,
    bound: usize,
    denial: C,
) -> wasmtime::Result<(StreamReader<u8>, RelayState<C>)>
where
    T: MiddlewareView + 'static,
    M: 'static,
    C: Send + 'static,
{
    let shared = Arc::new(Mutex::new(Shared {
        bytes: VecDeque::with_capacity(bound),
        in_flight: 0,
        bound,
        input_closed: false,
        output_closed: false,
        completion: None,
        consumer_waker: None,
        producer_waker: None,
        future_waker: None,
        denial: Some(denial),
        denied: false,
    }));
    input.pipe(
        store.as_context_mut(),
        Consumer {
            shared: Arc::clone(&shared),
            origin,
        },
    )?;
    let output = StreamReader::new(
        store.as_context_mut(),
        Producer {
            shared: Arc::clone(&shared),
        },
    )?;
    Ok((output, shared))
}

pub(crate) fn relay_completion<T, M, C>(
    store: &mut Access<'_, T, GateData<T, M>>,
    input: FutureReader<C>,
    shared: RelayState<C>,
) -> wasmtime::Result<FutureReader<C>>
where
    T: 'static,
    M: 'static,
    C: ComponentType + Lift + Lower + Send + Sync + 'static,
{
    input.pipe(
        store.as_context_mut(),
        CompletionConsumer {
            shared: Arc::clone(&shared),
        },
    )?;
    FutureReader::new(store.as_context_mut(), CompletionProducer { shared })
}
