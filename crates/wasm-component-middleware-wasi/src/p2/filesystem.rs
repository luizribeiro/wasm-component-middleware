use wasm_component_middleware::{ArgumentValue, MiddlewareView};
use wasmtime::component::{Linker, Resource};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::filesystem::{Descriptor, WasiFilesystemView as _};
use wasmtime_wasi::p2::bindings::filesystem::{preopens, types as async_types};
use wasmtime_wasi::p2::bindings::sync::filesystem::types;
use wasmtime_wasi::p2::bindings::sync::io::streams::{self, InputStream, OutputStream};
use wasmtime_wasi::p2::{FsError, FsResult};

use super::WASI_VERSION;
use super::gate::{Gate, GateData, gate, produced_directories, produced_resource, project};

macro_rules! filesystem {
    ($gate:ident, $function:literal, handles = [$($handle:expr),* $(,)?], args = $args:tt, delegate = $delegate:expr $(, produced = $produced:expr)?) => {
        gate!(filesystem $gate, WASI_VERSION, "wasi:filesystem/types", $function,
            handles = [$($handle),*], args = $args, delegate = $delegate,
            denied = async_types::ErrorCode::Access.into(), wrapper = FsError
            $(, produced = $produced)?)
    };
}

impl<T> types::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn convert_error_code(&mut self, error: FsError) -> wasmtime::Result<types::ErrorCode> {
        types::Host::convert_error_code(&mut self.state.filesystem(), error)
    }

    fn filesystem_error_code(
        &mut self,
        error: Resource<streams::Error>,
    ) -> wasmtime::Result<Option<types::ErrorCode>> {
        gate!(trap self, WASI_VERSION, "wasi:filesystem/types", "filesystem-error-code", handles = [error], args = (), delegate = |state: &mut T| types::Host::filesystem_error_code(&mut state.filesystem(), error))
    }
}

impl<T> types::HostDescriptor for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn advise(
        &mut self,
        fd: Resource<Descriptor>,
        offset: u64,
        length: u64,
        advice: types::Advice,
    ) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.advise",
            handles = [fd],
            args = [
                offset = offset,
                length = length,
                advice = ArgumentValue::Debug(format!("{advice:?}"))
            ],
            delegate = |state: &mut T| types::HostDescriptor::advise(
                &mut state.filesystem(),
                fd,
                offset,
                length,
                advice
            )
        )
    }

    fn sync_data(&mut self, fd: Resource<Descriptor>) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.sync-data",
            handles = [fd],
            args = (),
            delegate =
                |state: &mut T| types::HostDescriptor::sync_data(&mut state.filesystem(), fd)
        )
    }

    fn get_flags(&mut self, fd: Resource<Descriptor>) -> FsResult<types::DescriptorFlags> {
        filesystem!(
            self,
            "[method]descriptor.get-flags",
            handles = [fd],
            args = (),
            delegate =
                |state: &mut T| types::HostDescriptor::get_flags(&mut state.filesystem(), fd)
        )
    }

    fn get_type(&mut self, fd: Resource<Descriptor>) -> FsResult<types::DescriptorType> {
        filesystem!(
            self,
            "[method]descriptor.get-type",
            handles = [fd],
            args = (),
            delegate = |state: &mut T| types::HostDescriptor::get_type(&mut state.filesystem(), fd)
        )
    }

    fn set_size(&mut self, fd: Resource<Descriptor>, size: u64) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.set-size",
            handles = [fd],
            args = [size = size],
            delegate =
                |state: &mut T| types::HostDescriptor::set_size(&mut state.filesystem(), fd, size)
        )
    }

    fn set_times(
        &mut self,
        fd: Resource<Descriptor>,
        atime: types::NewTimestamp,
        mtime: types::NewTimestamp,
    ) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.set-times",
            handles = [fd],
            args = [
                data_access_timestamp = ArgumentValue::Debug(format!("{atime:?}")),
                data_modification_timestamp = ArgumentValue::Debug(format!("{mtime:?}"))
            ],
            delegate = |state: &mut T| types::HostDescriptor::set_times(
                &mut state.filesystem(),
                fd,
                atime,
                mtime
            )
        )
    }

    fn read(
        &mut self,
        fd: Resource<Descriptor>,
        length: u64,
        offset: u64,
    ) -> FsResult<(Vec<u8>, bool)> {
        filesystem!(
            self,
            "[method]descriptor.read",
            handles = [fd],
            args = [length = length, offset = offset],
            delegate = |state: &mut T| types::HostDescriptor::read(
                &mut state.filesystem(),
                fd,
                length,
                offset
            )
        )
    }

    fn write(&mut self, fd: Resource<Descriptor>, buffer: Vec<u8>, offset: u64) -> FsResult<u64> {
        filesystem!(
            self,
            "[method]descriptor.write",
            handles = [fd],
            args = [buffer = ArgumentValue::bytes(&buffer), offset = offset],
            delegate = |state: &mut T| types::HostDescriptor::write(
                &mut state.filesystem(),
                fd,
                buffer,
                offset
            )
        )
    }

    fn read_directory(
        &mut self,
        fd: Resource<Descriptor>,
    ) -> FsResult<Resource<types::DirectoryEntryStream>> {
        filesystem!(
            self,
            "[method]descriptor.read-directory",
            handles = [fd],
            args = (),
            delegate =
                |state: &mut T| types::HostDescriptor::read_directory(&mut state.filesystem(), fd),
            produced = |value: &Resource<types::DirectoryEntryStream>| vec![value.rep()]
        )
    }

    fn sync(&mut self, fd: Resource<Descriptor>) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.sync",
            handles = [fd],
            args = (),
            delegate = |state: &mut T| types::HostDescriptor::sync(&mut state.filesystem(), fd)
        )
    }

    fn create_directory_at(&mut self, fd: Resource<Descriptor>, path: String) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.create-directory-at",
            handles = [fd],
            args = [path = path.clone()],
            delegate = |state: &mut T| types::HostDescriptor::create_directory_at(
                &mut state.filesystem(),
                fd,
                path
            )
        )
    }

    fn stat(&mut self, fd: Resource<Descriptor>) -> FsResult<types::DescriptorStat> {
        filesystem!(
            self,
            "[method]descriptor.stat",
            handles = [fd],
            args = (),
            delegate = |state: &mut T| types::HostDescriptor::stat(&mut state.filesystem(), fd)
        )
    }

    fn stat_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: types::PathFlags,
        path: String,
    ) -> FsResult<types::DescriptorStat> {
        filesystem!(
            self,
            "[method]descriptor.stat-at",
            handles = [fd],
            args = [
                path_flags = ArgumentValue::Debug(format!("{path_flags:?}")),
                path = path.clone()
            ],
            delegate = |state: &mut T| types::HostDescriptor::stat_at(
                &mut state.filesystem(),
                fd,
                path_flags,
                path
            )
        )
    }

    fn set_times_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: types::PathFlags,
        path: String,
        atime: types::NewTimestamp,
        mtime: types::NewTimestamp,
    ) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.set-times-at",
            handles = [fd],
            args = [
                path_flags = ArgumentValue::Debug(format!("{path_flags:?}")),
                path = path.clone(),
                data_access_timestamp = ArgumentValue::Debug(format!("{atime:?}")),
                data_modification_timestamp = ArgumentValue::Debug(format!("{mtime:?}"))
            ],
            delegate = |state: &mut T| types::HostDescriptor::set_times_at(
                &mut state.filesystem(),
                fd,
                path_flags,
                path,
                atime,
                mtime
            )
        )
    }

    fn link_at(
        &mut self,
        fd: Resource<Descriptor>,
        old_path_flags: types::PathFlags,
        old_path: String,
        new_fd: Resource<Descriptor>,
        new_path: String,
    ) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.link-at",
            handles = [fd, new_fd],
            args = [
                old_path_flags = ArgumentValue::Debug(format!("{old_path_flags:?}")),
                old_path = old_path.clone(),
                new_path = new_path.clone()
            ],
            delegate = |state: &mut T| types::HostDescriptor::link_at(
                &mut state.filesystem(),
                fd,
                old_path_flags,
                old_path,
                new_fd,
                new_path
            )
        )
    }

    fn open_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: types::PathFlags,
        path: String,
        open_flags: types::OpenFlags,
        flags: types::DescriptorFlags,
    ) -> FsResult<Resource<Descriptor>> {
        filesystem!(
            self,
            "[method]descriptor.open-at",
            handles = [fd],
            args = [
                path_flags = ArgumentValue::Debug(format!("{path_flags:?}")),
                path = path.clone(),
                open_flags = ArgumentValue::Debug(format!("{open_flags:?}")),
                flags = ArgumentValue::Debug(format!("{flags:?}"))
            ],
            delegate = |state: &mut T| types::HostDescriptor::open_at(
                &mut state.filesystem(),
                fd,
                path_flags,
                path,
                open_flags,
                flags
            ),
            produced = produced_resource
        )
    }

    fn drop(&mut self, fd: Resource<Descriptor>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:filesystem/types", "[resource-drop]descriptor", handles = [fd], args = (), delegate = |state: &mut T| types::HostDescriptor::drop(&mut state.filesystem(), fd))
    }

    fn readlink_at(&mut self, fd: Resource<Descriptor>, path: String) -> FsResult<String> {
        filesystem!(
            self,
            "[method]descriptor.readlink-at",
            handles = [fd],
            args = [path = path.clone()],
            delegate = |state: &mut T| types::HostDescriptor::readlink_at(
                &mut state.filesystem(),
                fd,
                path
            )
        )
    }

    fn remove_directory_at(&mut self, fd: Resource<Descriptor>, path: String) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.remove-directory-at",
            handles = [fd],
            args = [path = path.clone()],
            delegate = |state: &mut T| types::HostDescriptor::remove_directory_at(
                &mut state.filesystem(),
                fd,
                path
            )
        )
    }

    fn rename_at(
        &mut self,
        fd: Resource<Descriptor>,
        old_path: String,
        new_fd: Resource<Descriptor>,
        new_path: String,
    ) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.rename-at",
            handles = [fd, new_fd],
            args = [old_path = old_path.clone(), new_path = new_path.clone()],
            delegate = |state: &mut T| types::HostDescriptor::rename_at(
                &mut state.filesystem(),
                fd,
                old_path,
                new_fd,
                new_path
            )
        )
    }

    fn symlink_at(
        &mut self,
        fd: Resource<Descriptor>,
        old_path: String,
        new_path: String,
    ) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.symlink-at",
            handles = [fd],
            args = [old_path = old_path.clone(), new_path = new_path.clone()],
            delegate = |state: &mut T| types::HostDescriptor::symlink_at(
                &mut state.filesystem(),
                fd,
                old_path,
                new_path
            )
        )
    }

    fn unlink_file_at(&mut self, fd: Resource<Descriptor>, path: String) -> FsResult<()> {
        filesystem!(
            self,
            "[method]descriptor.unlink-file-at",
            handles = [fd],
            args = [path = path.clone()],
            delegate = |state: &mut T| types::HostDescriptor::unlink_file_at(
                &mut state.filesystem(),
                fd,
                path
            )
        )
    }

    fn read_via_stream(
        &mut self,
        fd: Resource<Descriptor>,
        offset: u64,
    ) -> FsResult<Resource<InputStream>> {
        filesystem!(
            self,
            "[method]descriptor.read-via-stream",
            handles = [fd],
            args = [offset = offset],
            delegate = |state: &mut T| types::HostDescriptor::read_via_stream(
                &mut state.filesystem(),
                fd,
                offset
            ),
            produced = |value: &Resource<InputStream>| vec![value.rep()]
        )
    }

    fn write_via_stream(
        &mut self,
        fd: Resource<Descriptor>,
        offset: u64,
    ) -> FsResult<Resource<OutputStream>> {
        filesystem!(
            self,
            "[method]descriptor.write-via-stream",
            handles = [fd],
            args = [offset = offset],
            delegate = |state: &mut T| types::HostDescriptor::write_via_stream(
                &mut state.filesystem(),
                fd,
                offset
            ),
            produced = |value: &Resource<OutputStream>| vec![value.rep()]
        )
    }

    fn append_via_stream(&mut self, fd: Resource<Descriptor>) -> FsResult<Resource<OutputStream>> {
        filesystem!(
            self,
            "[method]descriptor.append-via-stream",
            handles = [fd],
            args = (),
            delegate = |state: &mut T| types::HostDescriptor::append_via_stream(
                &mut state.filesystem(),
                fd
            ),
            produced = |value: &Resource<OutputStream>| vec![value.rep()]
        )
    }

    fn is_same_object(
        &mut self,
        fd: Resource<Descriptor>,
        other: Resource<Descriptor>,
    ) -> wasmtime::Result<bool> {
        gate!(trap self, WASI_VERSION, "wasi:filesystem/types", "[method]descriptor.is-same-object", handles = [fd, other], args = (), delegate = |state: &mut T| types::HostDescriptor::is_same_object(&mut state.filesystem(), fd, other))
    }

    fn metadata_hash(&mut self, fd: Resource<Descriptor>) -> FsResult<types::MetadataHashValue> {
        filesystem!(
            self,
            "[method]descriptor.metadata-hash",
            handles = [fd],
            args = (),
            delegate =
                |state: &mut T| types::HostDescriptor::metadata_hash(&mut state.filesystem(), fd)
        )
    }

    fn metadata_hash_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: types::PathFlags,
        path: String,
    ) -> FsResult<types::MetadataHashValue> {
        filesystem!(
            self,
            "[method]descriptor.metadata-hash-at",
            handles = [fd],
            args = [
                path_flags = ArgumentValue::Debug(format!("{path_flags:?}")),
                path = path.clone()
            ],
            delegate = |state: &mut T| types::HostDescriptor::metadata_hash_at(
                &mut state.filesystem(),
                fd,
                path_flags,
                path
            )
        )
    }
}

impl<T> types::HostDirectoryEntryStream for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn read_directory_entry(
        &mut self,
        stream: Resource<types::DirectoryEntryStream>,
    ) -> FsResult<Option<types::DirectoryEntry>> {
        filesystem!(
            self,
            "[method]directory-entry-stream.read-directory-entry",
            handles = [stream],
            args = (),
            delegate = |state: &mut T| types::HostDirectoryEntryStream::read_directory_entry(
                &mut state.filesystem(),
                stream
            )
        )
    }

    fn drop(&mut self, stream: Resource<types::DirectoryEntryStream>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:filesystem/types", "[resource-drop]directory-entry-stream", handles = [stream], args = (), delegate = |state: &mut T| types::HostDirectoryEntryStream::drop(&mut state.filesystem(), stream))
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
    preopens::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    types::add_to_linker::<T, GateData<T>>(linker, project::<T>)
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
        produced: Vec<u32>,
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

    struct RecordProduced;

    impl Layer<State> for RecordProduced {
        type Frame = ();

        fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
            assert_eq!(call.function, "get-directories");
            Ok(())
        }

        fn after(&self, state: &mut State, _call: &Call<'_>, (): (), outcome: Outcome<'_>) {
            if let Outcome::Returned(completion) = outcome {
                state.produced.clone_from(&completion.produced);
            }
        }
    }

    #[test]
    fn preopens_dispatch_and_report_descriptors() {
        let chain = Chain::builder().layer(RecordProduced).build();
        let mut builder = WasiCtxBuilder::new();
        builder.preopened_dir(".", ".", FsPerms::ReadOnly).unwrap();
        let mut state = State {
            middleware: Some(MiddlewareCtx::new(
                chain,
                InvocationContext::new("filesystem"),
            )),
            table: ResourceTable::new(),
            wasi: builder.build(),
            produced: Vec::new(),
        };

        let directories = preopens::Host::get_directories(&mut project(&mut state)).unwrap();

        assert_eq!(directories.len(), 1);
        assert_eq!(state.produced.len(), 1);
        assert_eq!(state.produced[0], directories[0].0.rep());
    }
}
