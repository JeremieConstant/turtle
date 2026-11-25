//! The address Turtle listens on (`TURTLE_HOST`): loopback unless the
//! operator says otherwise, as inside a container.

mod support;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use support::TestServer;

#[tokio::test]
async fn without_a_host_turtle_listens_on_loopback_only() {
    let server = TestServer::start().await;
    assert_eq!(server.addr().ip(), IpAddr::from(Ipv4Addr::LOCALHOST));
    server.sign_up().await;
}

#[tokio::test]
async fn turtle_listens_on_the_address_turtle_host_names() {
    let server = TestServer::start_listening_on("::1").await;
    assert_eq!(server.addr().ip(), IpAddr::from(Ipv6Addr::LOCALHOST));
    server.sign_up().await;
}

#[tokio::test]
async fn on_every_address_turtle_is_reached_through_loopback() {
    let server = TestServer::start_listening_on("0.0.0.0").await;
    server.sign_up().await;
}

#[tokio::test]
async fn turtle_refuses_a_host_that_is_not_an_address() {
    let error = TestServer::refuses_to_start_listening_on("turtle.example.org").await;
    assert!(error.contains("TURTLE_HOST"), "{error}");
    assert!(error.contains("turtle.example.org"), "{error}");
}
