pub(super) use crate::gate::{
    Gate, GateData, gate, produced_directories, produced_resource, project,
};

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use wit_parser::Resolve;

    use super::super::WASI_VERSION;

    #[test]
    fn reported_version_matches_every_vendored_dependency() {
        let wit = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wit");
        let dependency_count = fs::read_dir(wit.join("deps"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "wit")
            })
            .count();
        let mut resolve = Resolve::default();
        resolve.push_dir(wit).unwrap();
        let versions = resolve
            .packages
            .iter()
            .filter_map(|(_, package)| package.name.version.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(versions.len(), dependency_count);
        assert!(
            versions
                .iter()
                .all(|version| version.to_string() == WASI_VERSION)
        );
    }
}
