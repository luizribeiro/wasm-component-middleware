wit_bindgen::generate!({
    path: "wit",
    world: "unrouted-import",
});

use example::unrouted::host::{classified, secret};

struct Component;

impl Guest for Component {
    fn reveal() -> String {
        format!("{}{}", secret(), classified())
    }
}

export!(Component);
