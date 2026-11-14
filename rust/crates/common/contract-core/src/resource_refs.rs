use std::fs;
use std::path::{Component, Path};

use serde_json::Value;

pub fn expand(value: &mut Value, root: &Path) {
    if let Some(reference) = value.get("$ref").and_then(Value::as_str).map(str::to_owned) {
        if reference.starts_with('#') {
            return;
        }
        assert_eq!(
            value.as_object().map(|object| object.len()),
            Some(1),
            "reference must be the only field"
        );
        let relative = Path::new(&reference);
        assert!(
            !relative.is_absolute()
                && relative
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
                && relative
                    .extension()
                    .is_some_and(|extension| extension == "json"),
            "invalid contract resource reference: {reference}"
        );
        let path = root.join(relative);
        println!("cargo:rerun-if-changed={}", path.display());
        *value = serde_json::from_slice(
            &fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())),
        )
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            value.get("$ref").is_none(),
            "nested resource reference is unsupported: {reference}"
        );
        return;
    }
    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                expand(child, root);
            }
        }
        Value::Array(array) => {
            for child in array {
                expand(child, root);
            }
        }
        _ => {}
    }
}
