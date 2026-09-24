wit_bindgen::generate!({
    path: "wit",
    world: "smoke-p3",
});

use exports::test::smoke_p3::greeter::Guest;

struct Component;

impl Guest for Component {
    async fn greet(name: String) -> String {
        format!("Hello, {name}!")
    }
}

export!(Component);
