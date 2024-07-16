use super::{is_reserved_word, Config};
use std::collections::HashMap;

#[test]
fn when_reserved_word() {
    assert!(is_reserved_word("end"));
}

#[test]
fn when_not_reserved_word() {
    assert!(!is_reserved_word("ruby"));
}

#[test]
fn cdylib_name() {
    let config = Config {
        cdylib_name: None,
        cdylib_path: None,
        rename: HashMap::default(),
    };

    assert_eq!("uniffi", config.cdylib_name());

    let config = Config {
        cdylib_name: Some("todolist".to_string()),
        cdylib_path: None,
        rename: HashMap::default(),
    };

    assert_eq!("todolist", config.cdylib_name());
}

#[test]
fn cdylib_path() {
    let config = Config {
        cdylib_name: None,
        cdylib_path: None,
        rename: HashMap::default(),
    };

    assert_eq!("", config.cdylib_path());
    assert!(!config.custom_cdylib_path());

    let config = Config {
        cdylib_name: None,
        cdylib_path: Some("/foo/bar".to_string()),
        rename: HashMap::default(),
    };

    assert_eq!("/foo/bar", config.cdylib_path());
    assert!(config.custom_cdylib_path());
}
