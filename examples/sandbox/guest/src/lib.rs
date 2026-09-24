wit_bindgen::generate!({
    path: "wit",
    world: "sandbox",
});

use std::fs::File;

struct Component;

impl Guest for Component {
    fn run() {
        println!(
            "read public/note.txt: {}",
            std::fs::read_to_string("public/note.txt").unwrap()
        );
        report_refusal("private/secret.txt", "read private/secret.txt");
        report_refusal("public/../private/secret.txt", "escape from public preopen");

        let mut files = Vec::new();
        for path in ["public/one.txt", "public/two.txt", "public/three.txt"] {
            match File::open(path) {
                Ok(file) => {
                    files.push(file);
                    println!("open {path}: allowed");
                }
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                    println!("open {path}: access");
                }
                Err(error) => println!("open {path}: unexpected {error}"),
            }
        }
        drop(files);
    }
}

export!(Component);

fn report_refusal(path: &str, label: &str) {
    match std::fs::read_to_string(path) {
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            println!("{label}: access");
        }
        result => println!("{label}: unexpected {result:?}"),
    }
}
