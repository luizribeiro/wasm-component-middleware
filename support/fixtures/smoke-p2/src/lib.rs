wit_bindgen::generate!({
    path: "wit",
    world: "smoke-p2",
});

use exports::test::smoke_p2::greeter::Guest;

struct Component;

impl Guest for Component {
    fn greet(name: String) -> String {
        format!("Hello, {name}!")
    }
}

export!(Component);
