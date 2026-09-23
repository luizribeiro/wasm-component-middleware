wit_bindgen::generate!({
    path: "../../crates/wasm-component-middleware-wasi/wit",
    world: "network-error-code-probe",
    features: ["network-error-code"],
    generate_all,
});

struct Component;

impl Guest for Component {
    fn probe() -> bool {
        let stdout = wasi::cli::stdout::get_stdout();
        let Err(wasi::io::streams::StreamError::LastOperationFailed(error)) =
            stdout.blocking_write_and_flush(b"probe")
        else {
            return false;
        };
        let _ = wasi::sockets::network::network_error_code(&error);
        true
    }
}

export!(Component);
