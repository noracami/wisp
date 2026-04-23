use wisp::tpp_poc::PocState;

#[tokio::test]
async fn poc_state_starts_empty() {
    let state = PocState::new();
    let webhooks = state.webhooks.read().await;
    assert!(webhooks.is_empty());
}

#[tokio::test]
async fn poc_state_insert_and_read() {
    let state = PocState::new();
    state
        .webhooks
        .write()
        .await
        .insert("user-1".to_string(), "https://webhook.example/x".to_string());

    let webhooks = state.webhooks.read().await;
    assert_eq!(
        webhooks.get("user-1"),
        Some(&"https://webhook.example/x".to_string())
    );
}
