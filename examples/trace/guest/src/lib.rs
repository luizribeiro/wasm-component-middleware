wit_bindgen::generate!({
    path: "wit",
    world: "hello",
});

use example::hello::host::{log, user_name};

struct Component;

impl Guest for Component {
    fn greet(greeting: String) -> String {
        let name = user_name();
        log(&format!("greeting {name}"));
        format!("{greeting}, {name}!")
    }
}

export!(Component);
