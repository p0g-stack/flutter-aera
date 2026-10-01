//! flutter-aera: the Flutter embedder for AERA Recovery's generic pixel +
//! GPU plugin host. See README.md for the shape and spec/host.md for the
//! host contract.

#[allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code, clippy::all)]
pub mod ffi {
    include!(concat!(env!("OUT_DIR"), "/flutter_embedder.rs"));
}

pub mod aera_settings;
pub mod audio;
pub mod debug;
pub mod engine;
pub mod env;
pub mod handlers;
pub mod host;
pub mod ime;
pub mod renderer;
pub mod task_runner;
pub mod view;
