//! Shared by the tests that drive Turtle from the outside, and meant to be
//! reused by the load test.
// Each test file uses a different part of this.
#![allow(dead_code)]

mod client;
pub mod http;
mod server;

#[allow(unused_imports)]
pub use client::{Content, TestClient};
#[allow(unused_imports)]
pub use server::{Member, TestServer};
