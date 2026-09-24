wit_bindgen::generate!({
    path: "wit",
    world: "cat-p2",
    generate_all,
});

struct Component;

impl exports::wasi::cli::run::Guest for Component {
    fn run() -> Result<(), ()> {
        let argument = wasi::cli::environment::get_arguments()
            .into_iter()
            .nth(1)
            .ok_or(())?;
        if let Some(code) = argument.strip_prefix("--exit=") {
            let code = code.parse::<u8>().map_err(|_| ())?;
            if code == 0 {
                wasi::cli::exit::exit(Ok(()));
            } else {
                wasi::cli::exit::exit_with_code(code);
            }
            return Ok(());
        }

        let (root, _) = wasi::filesystem::preopens::get_directories()
            .into_iter()
            .next()
            .ok_or(())?;
        let file = root
            .open_at(
                wasi::filesystem::types::PathFlags::empty(),
                &argument,
                wasi::filesystem::types::OpenFlags::empty(),
                wasi::filesystem::types::DescriptorFlags::READ,
            )
            .map_err(|_| ())?;
        let (contents, _) = file.read(u64::MAX, 0).map_err(|_| ())?;
        let stdout = wasi::cli::stdout::get_stdout();
        for chunk in contents.chunks(4096) {
            stdout.blocking_write_and_flush(chunk).map_err(|_| ())?;
        }
        Ok(())
    }
}

export!(Component);
