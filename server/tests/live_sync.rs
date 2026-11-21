//! Live synchronisation, tested from the outside: the real server on a free
//! port with its own data directory, and clients speaking the editor's frame
//! protocol over WebSocket.
//!
//! Unix only: the server is stopped with SIGTERM, as a service manager would.
#![cfg(unix)]

mod support;

use support::TestServer;

// One thread, so the two edits below are really concurrent (see there).
#[tokio::test(flavor = "current_thread")]
async fn two_clients_editing_at_the_same_time_end_with_the_same_content() {
    let mut server = TestServer::start().await;
    // Editors are told apart by their connection, not their account, so
    // every client here signs in as the same Member.
    let member = server.sign_up().await;
    let page = member.create_page("Shared notes").await;
    let alice = server.connect(&member, &page).await;
    let bob = server.connect(&member, &page).await;
    alice.append_paragraph("Q1 goals");
    bob.wait_until("bob sees the paragraph", |content| {
        content.paragraphs == ["Q1 goals"]
    })
    .await;

    // Both edit before either has seen the other's change. The test runtime
    // has one thread, so nothing is received between these two lines.
    alice.type_text(0, 0, "Draft: ");
    bob.type_text(0, 8, " and risks");

    let expected = ["Draft: Q1 goals and risks"];
    let seen_by_alice = alice
        .wait_until("alice sees both edits", |content| content.paragraphs == expected)
        .await;
    let seen_by_bob = bob
        .wait_until("bob sees both edits", |content| content.paragraphs == expected)
        .await;
    assert_eq!(seen_by_alice.document, seen_by_bob.document);

    // Someone who opens the Page afterwards gets the same.
    let carol = server.connect(&member, &page).await;
    assert_eq!(carol.content().document, seen_by_alice.document);
    server.stop().await;
}

#[tokio::test]
async fn content_survives_a_restart_on_the_same_data_directory() {
    let mut server = TestServer::start().await;
    // Editors are told apart by their connection, not their account, so
    // every client here signs in as the same Member.
    let member = server.sign_up().await;
    let page = member.create_page("Shared notes").await;
    let writer = server.connect(&member, &page).await;
    let observer = server.connect(&member, &page).await;
    writer.append_paragraph("Day one: laptop and accounts.");
    writer.type_text(0, 0, "Checklist. ");
    observer
        .wait_until("the server has both edits", |content| {
            content.paragraphs == ["Checklist. Day one: laptop and accounts."]
        })
        .await;
    let before = observer.content().document;

    // Stopped well within the save interval: the last save happens on the way out.
    server.restart().await;

    let reader = server.connect(&member, &page).await;
    assert_eq!(reader.paragraphs(), ["Checklist. Day one: laptop and accounts."]);
    assert_eq!(reader.content().document, before);
    server.stop().await;
}

#[tokio::test]
async fn the_others_are_told_when_someone_leaves() {
    let mut server = TestServer::start().await;
    // Editors are told apart by their connection, not their account, so
    // every client here signs in as the same Member.
    let member = server.sign_up().await;
    let page = member.create_page("Shared notes").await;
    let alice = server.connect(&member, &page).await;
    alice.set_presence(r#"{"user":{"name":"Alice"},"cursor":null}"#);
    let bob = server.connect(&member, &page).await;
    bob.set_presence(r#"{"user":{"name":"Bob"},"cursor":null}"#);

    // Bob joined after Alice announced herself; he learns of her because the
    // server asks everyone to announce themselves when someone joins.
    let (alice_id, bob_id) = (alice.client_id(), bob.client_id());
    bob.wait_until("bob sees alice", |content| {
        content.others.get(&alice_id).is_some_and(|state| state.contains("Alice"))
    })
    .await;
    alice
        .wait_until("alice sees bob", |content| content.others.contains_key(&bob_id))
        .await;

    // Bob closes the connection without saying goodbye first, as a crashed
    // tab would: the server has to announce it.
    bob.disconnect().await;
    alice
        .wait_until("alice is told bob is gone", |content| content.others.is_empty())
        .await;
    server.stop().await;
}

#[tokio::test]
async fn a_client_that_joins_later_receives_the_full_content() {
    let mut server = TestServer::start().await;
    // Editors are told apart by their connection, not their account, so
    // every client here signs in as the same Member.
    let member = server.sign_up().await;
    let page = member.create_page("Shared notes").await;
    let writer = server.connect(&member, &page).await;
    let observer = server.connect(&member, &page).await;

    writer.append_paragraph("Welcome to the handbook.");
    writer.append_paragraph("Ask in the team channel.");
    // The server applies an update before passing it on, so once the observer
    // has it, the server has it too.
    observer
        .wait_until("the observer sees both paragraphs", |content| {
            content.paragraphs.len() == 2
        })
        .await;

    let late = server.connect(&member, &page).await;
    // No waiting: everything has to arrive with the state sent on joining.
    assert_eq!(
        late.paragraphs(),
        ["Welcome to the handbook.", "Ask in the team channel."]
    );
    server.stop().await;
}
