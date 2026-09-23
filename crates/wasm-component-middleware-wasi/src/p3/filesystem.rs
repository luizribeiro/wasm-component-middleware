use wasm_component_middleware::{ArgumentValue, MiddlewareView};
use wasmtime::AsContextMut;
use wasmtime::component::{Access, Accessor, FutureReader, Linker, Resource, StreamReader};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::filesystem::{Descriptor, WasiFilesystem, WasiFilesystemView as _};
use wasmtime_wasi::p3::bindings::filesystem::{preopens, types};
use wasmtime_wasi::p3::filesystem::{FilesystemError, FilesystemResult};

use crate::gate::{Gate, GateData, gate, produced_directories, produced_resource, project};

use super::WASI_VERSION;
use super::relay::{Origin, RelayMode, Relayed, relay_bytes, relay_completion};

fn delegate_access<'a, T, M>(
    store: &'a mut Access<'_, T, GateData<T, M>>,
) -> Access<'a, T, WasiFilesystem>
where
    T: WasiView + MiddlewareView + 'static,
    M: 'static,
{
    Access::new(store.as_context_mut(), |state: &mut T| state.filesystem())
}

macro_rules! filesystem_async {
    ($store:ident, $function:literal, handles = $handles:expr, args = $args:tt, delegate = $delegate:expr $(, produced = $produced:expr)?) => {
        gate!(filesystem_async $store, "wasi:filesystem/types", $function,
            handles = $handles, args = $args, delegate = $delegate,
            denied = types::ErrorCode::Access.into(), wrapper = FilesystemError
            $(, produced = $produced)?)
    };
}

impl<T> types::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn convert_error_code(&mut self, error: FilesystemError) -> wasmtime::Result<types::ErrorCode> {
        types::Host::convert_error_code(&mut self.state.filesystem(), error)
    }
}

impl<T> types::HostDescriptor for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn drop(&mut self, fd: Resource<Descriptor>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:filesystem/types", "[resource-drop]descriptor", handles = [fd], args = (), delegate = |state: &mut T| types::HostDescriptor::drop(&mut state.filesystem(), fd))
    }
}

impl<T, M> types::HostDescriptorWithStore<T> for GateData<T, M>
where
    T: WasiView + MiddlewareView + 'static,
    M: RelayMode,
{
    fn read_via_stream(
        mut store: Access<T, Self>,
        fd: Resource<Descriptor>,
        offset: u64,
    ) -> wasmtime::Result<(StreamReader<u8>, FutureReader<Result<(), types::ErrorCode>>)> {
        let Some(capacity) = M::CAPACITY else {
            return gate!(access store, "wasi:filesystem/types", "[method]descriptor.read-via-stream", handles = [fd.rep()], args = [offset = offset], delegate = |store| types::HostDescriptorWithStore::read_via_stream(delegate_access(store), fd, offset));
        };
        let descriptor = fd.rep();
        let chain = std::sync::Arc::clone(store.data_mut().middleware().chain());
        let handles = [descriptor];
        let arguments = wasm_component_middleware::Arguments::new().with("offset", offset);
        let call = wasm_component_middleware::Call::new(
            chain.next_id(),
            wasm_component_middleware::Direction::Import,
            "[method]descriptor.read-via-stream",
        )
        .in_interface("wasi:filesystem/types", Some(WASI_VERSION))
        .with_handles(&handles)
        .with_args(&arguments);
        chain.dispatch_access(store, &call, |store| {
            let (input, completion) = types::HostDescriptorWithStore::read_via_stream(
                delegate_access(store),
                fd,
                offset,
            )?;
            let shared_origin = Origin {
                call_id: call.id,
                interface: "wasi:filesystem/types",
                version: WASI_VERSION,
                function: "[stream-read]read-via-stream",
                handles: std::sync::Arc::from([descriptor]),
            };
            let (output, shared) = relay_bytes(
                store,
                input,
                shared_origin,
                capacity,
                Err(types::ErrorCode::Access),
            )?;
            let completion = relay_completion(store, completion, shared)?;
            Ok((
                (output, completion),
                wasm_component_middleware::Completion::default(),
            ))
        })
    }

    fn write_via_stream(
        mut store: Access<'_, T, Self>,
        fd: Resource<Descriptor>,
        data: StreamReader<u8>,
        offset: u64,
    ) -> wasmtime::Result<FutureReader<Result<(), types::ErrorCode>>> {
        gate!(access store, "wasi:filesystem/types", "[method]descriptor.write-via-stream", handles = [fd.rep()], args = [offset = offset], delegate = |store| types::HostDescriptorWithStore::write_via_stream(delegate_access(store), fd, data, offset))
    }

    fn append_via_stream(
        mut store: Access<'_, T, Self>,
        fd: Resource<Descriptor>,
        data: StreamReader<u8>,
    ) -> wasmtime::Result<FutureReader<Result<(), types::ErrorCode>>> {
        gate!(access store, "wasi:filesystem/types", "[method]descriptor.append-via-stream", handles = [fd.rep()], args = (), delegate = |store| types::HostDescriptorWithStore::append_via_stream(delegate_access(store), fd, data))
    }

    async fn advise(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        offset: u64,
        length: u64,
        advice: types::Advice,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.advise",
            handles = [fd.rep()],
            args = [
                offset = offset,
                length = length,
                advice = ArgumentValue::Debug(format!("{advice:?}"))
            ],
            delegate =
                types::HostDescriptorWithStore::advise(&delegate, fd, offset, length, advice)
        )
    }

    async fn sync_data(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.sync-data",
            handles = [fd.rep()],
            args = (),
            delegate = types::HostDescriptorWithStore::sync_data(&delegate, fd)
        )
    }

    async fn get_flags(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
    ) -> FilesystemResult<types::DescriptorFlags> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.get-flags",
            handles = [fd.rep()],
            args = (),
            delegate = types::HostDescriptorWithStore::get_flags(&delegate, fd)
        )
    }

    async fn get_type(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
    ) -> FilesystemResult<types::DescriptorType> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.get-type",
            handles = [fd.rep()],
            args = (),
            delegate = types::HostDescriptorWithStore::get_type(&delegate, fd)
        )
    }

    async fn set_size(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        size: u64,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.set-size",
            handles = [fd.rep()],
            args = [size = size],
            delegate = types::HostDescriptorWithStore::set_size(&delegate, fd, size)
        )
    }

    async fn set_times(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        atime: types::NewTimestamp,
        mtime: types::NewTimestamp,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.set-times",
            handles = [fd.rep()],
            args = [
                data_access_timestamp = ArgumentValue::Debug(format!("{atime:?}")),
                data_modification_timestamp = ArgumentValue::Debug(format!("{mtime:?}"))
            ],
            delegate = types::HostDescriptorWithStore::set_times(&delegate, fd, atime, mtime)
        )
    }

    fn read_directory(
        mut store: Access<'_, T, Self>,
        fd: Resource<Descriptor>,
    ) -> wasmtime::Result<(
        StreamReader<types::DirectoryEntry>,
        FutureReader<Result<(), types::ErrorCode>>,
    )> {
        gate!(access store, "wasi:filesystem/types", "[method]descriptor.read-directory", handles = [fd.rep()], args = (), delegate = |store| types::HostDescriptorWithStore::read_directory(delegate_access(store), fd))
    }

    async fn sync(store: &Accessor<T, Self>, fd: Resource<Descriptor>) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.sync",
            handles = [fd.rep()],
            args = (),
            delegate = types::HostDescriptorWithStore::sync(&delegate, fd)
        )
    }

    async fn create_directory_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.create-directory-at",
            handles = [fd.rep()],
            args = [path = path.clone()],
            delegate = types::HostDescriptorWithStore::create_directory_at(&delegate, fd, path)
        )
    }

    async fn stat(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
    ) -> FilesystemResult<types::DescriptorStat> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.stat",
            handles = [fd.rep()],
            args = (),
            delegate = types::HostDescriptorWithStore::stat(&delegate, fd)
        )
    }

    async fn stat_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        path_flags: types::PathFlags,
        path: String,
    ) -> FilesystemResult<types::DescriptorStat> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.stat-at",
            handles = [fd.rep()],
            args = [
                path_flags = ArgumentValue::Debug(format!("{path_flags:?}")),
                path = path.clone()
            ],
            delegate = types::HostDescriptorWithStore::stat_at(&delegate, fd, path_flags, path)
        )
    }

    async fn set_times_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        path_flags: types::PathFlags,
        path: String,
        atime: types::NewTimestamp,
        mtime: types::NewTimestamp,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.set-times-at",
            handles = [fd.rep()],
            args = [
                path_flags = ArgumentValue::Debug(format!("{path_flags:?}")),
                path = path.clone(),
                data_access_timestamp = ArgumentValue::Debug(format!("{atime:?}")),
                data_modification_timestamp = ArgumentValue::Debug(format!("{mtime:?}"))
            ],
            delegate = types::HostDescriptorWithStore::set_times_at(
                &delegate, fd, path_flags, path, atime, mtime
            )
        )
    }

    async fn link_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        old_path_flags: types::PathFlags,
        old_path: String,
        new_fd: Resource<Descriptor>,
        new_path: String,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.link-at",
            handles = [fd.rep(), new_fd.rep()],
            args = [
                old_path_flags = ArgumentValue::Debug(format!("{old_path_flags:?}")),
                old_path = old_path.clone(),
                new_path = new_path.clone()
            ],
            delegate = types::HostDescriptorWithStore::link_at(
                &delegate,
                fd,
                old_path_flags,
                old_path,
                new_fd,
                new_path
            )
        )
    }

    async fn open_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        path_flags: types::PathFlags,
        path: String,
        open_flags: types::OpenFlags,
        flags: types::DescriptorFlags,
    ) -> FilesystemResult<Resource<Descriptor>> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.open-at",
            handles = [fd.rep()],
            args = [
                path_flags = ArgumentValue::Debug(format!("{path_flags:?}")),
                path = path.clone(),
                open_flags = ArgumentValue::Debug(format!("{open_flags:?}")),
                flags = ArgumentValue::Debug(format!("{flags:?}"))
            ],
            delegate = types::HostDescriptorWithStore::open_at(
                &delegate, fd, path_flags, path, open_flags, flags
            ),
            produced = produced_resource
        )
    }

    async fn readlink_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FilesystemResult<String> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.readlink-at",
            handles = [fd.rep()],
            args = [path = path.clone()],
            delegate = types::HostDescriptorWithStore::readlink_at(&delegate, fd, path)
        )
    }

    async fn remove_directory_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.remove-directory-at",
            handles = [fd.rep()],
            args = [path = path.clone()],
            delegate = types::HostDescriptorWithStore::remove_directory_at(&delegate, fd, path)
        )
    }

    async fn rename_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        old_path: String,
        new_fd: Resource<Descriptor>,
        new_path: String,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.rename-at",
            handles = [fd.rep(), new_fd.rep()],
            args = [old_path = old_path.clone(), new_path = new_path.clone()],
            delegate = types::HostDescriptorWithStore::rename_at(
                &delegate, fd, old_path, new_fd, new_path
            )
        )
    }

    async fn symlink_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        old_path: String,
        new_path: String,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.symlink-at",
            handles = [fd.rep()],
            args = [old_path = old_path.clone(), new_path = new_path.clone()],
            delegate =
                types::HostDescriptorWithStore::symlink_at(&delegate, fd, old_path, new_path)
        )
    }

    async fn unlink_file_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FilesystemResult<()> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.unlink-file-at",
            handles = [fd.rep()],
            args = [path = path.clone()],
            delegate = types::HostDescriptorWithStore::unlink_file_at(&delegate, fd, path)
        )
    }

    async fn is_same_object(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        other: Resource<Descriptor>,
    ) -> wasmtime::Result<bool> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        gate!(async store, "wasi:filesystem/types", "[method]descriptor.is-same-object", handles = [fd.rep(), other.rep()], args = (), delegate = types::HostDescriptorWithStore::is_same_object(&delegate, fd, other))
    }

    async fn metadata_hash(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
    ) -> FilesystemResult<types::MetadataHashValue> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.metadata-hash",
            handles = [fd.rep()],
            args = (),
            delegate = types::HostDescriptorWithStore::metadata_hash(&delegate, fd)
        )
    }

    async fn metadata_hash_at(
        store: &Accessor<T, Self>,
        fd: Resource<Descriptor>,
        path_flags: types::PathFlags,
        path: String,
    ) -> FilesystemResult<types::MetadataHashValue> {
        let delegate = store.with_getter::<WasiFilesystem>(|state: &mut T| state.filesystem());
        filesystem_async!(
            store,
            "[method]descriptor.metadata-hash-at",
            handles = [fd.rep()],
            args = [
                path_flags = ArgumentValue::Debug(format!("{path_flags:?}")),
                path = path.clone()
            ],
            delegate =
                types::HostDescriptorWithStore::metadata_hash_at(&delegate, fd, path_flags, path)
        )
    }
}

impl<T> preopens::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_directories(&mut self) -> wasmtime::Result<Vec<(Resource<Descriptor>, String)>> {
        gate!(trap self, WASI_VERSION, "wasi:filesystem/preopens", "get-directories", handles = [], args = (), delegate = |state: &mut T| preopens::Host::get_directories(&mut state.filesystem()), produced = produced_directories)
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    types::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    preopens::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}

pub(super) fn add_to_linker_relayed<T, const CAPACITY: usize>(
    linker: &mut Linker<T>,
) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    types::add_to_linker::<T, GateData<T, Relayed<CAPACITY>>>(linker, project::<T>)?;
    preopens::add_to_linker::<T, GateData<T, Relayed<CAPACITY>>>(linker, project::<T>)
}

#[cfg(test)]
mod tests {
    use wasm_component_middleware::{
        Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx, Outcome,
    };
    use wasmtime::component::ResourceTable;
    use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView};

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

        fn before(&self, state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
            assert_eq!(call.function, "get-directories");
            state.calls += 1;
            Ok(())
        }

        fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
    }

    #[test]
    fn preopens_dispatch_through_the_chain() {
        let chain = Chain::builder().layer(Count).build();
        let mut builder = WasiCtxBuilder::new();
        builder.preopened_dir(".", ".", FsPerms::ReadOnly).unwrap();
        let mut state = State {
            middleware: Some(MiddlewareCtx::new(
                chain,
                InvocationContext::new("filesystem"),
            )),
            table: ResourceTable::new(),
            wasi: builder.build(),
            calls: 0,
        };

        let directories = preopens::Host::get_directories(&mut project(&mut state)).unwrap();

        assert_eq!(directories.len(), 1);
        assert_eq!(state.calls, 1);
    }
}
