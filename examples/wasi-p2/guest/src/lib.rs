wit_bindgen::generate!({
    path: "wit",
    world: "workload",
});

use std::time::{SystemTime, UNIX_EPOCH};

struct Component;

impl Guest for Component {
    fn basic_system() {
        let greeting = std::env::var("GREETING").unwrap_or_default();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let line = format!(
            "{greeting} at {}.{:09}\n",
            now.as_secs(),
            now.subsec_nanos()
        );
        wasi::cli::stdout::get_stdout()
            .blocking_write_and_flush(line.as_bytes())
            .unwrap();
    }
}

export!(Component);
