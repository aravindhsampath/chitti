use chitti::gemini::{Client, InteractionInput, InteractionEvent, InteractionOutput, Role, Part, Tool, ThinkingLevel, CachedContent};
use dotenvy::dotenv;
use std::env;
use futures_util::StreamExt;

fn get_test_client() -> Client {
    dotenv().ok();
    let api_key = env::var("TEST_API_KEY")
        .expect("TEST_API_KEY must be set for integration tests");
    let model = env::var("GEMINI_MODEL")
        .unwrap_or_else(|_| "gemini-3-flash-preview".to_string());
    Client::new(api_key, model)
}

#[tokio::test]
async fn test_exhaustive_checklist() -> anyhow::Result<()> {
    let client = get_test_client();
    
    println!("1. Testing Stateful Conversation...");
    let r1 = client.interaction(InteractionInput::Text("My name is Phil.".to_string()))
        .store(true)
        .send()
        .await?;
    let id = r1.id.expect("ID should be present");
    
    let r2 = client.interaction(InteractionInput::Text("What is my name?".to_string()))
        .previous_interaction_id(id)
        .store(true)
        .send()
        .await?;
    let text = match r2.outputs.iter().find(|o| matches!(o, InteractionOutput::Text { .. })).unwrap() {
        InteractionOutput::Text { text } => text,
        _ => panic!("Expected text"),
    };
    println!("Model said: {}", text);
    assert!(text.to_lowercase().contains("phil"));

    println!("2. Testing Streaming & Content Delta...");
    let mut stream = client.interaction(InteractionInput::Text("Count to 3".to_string()))
        .stream()
        .await?;
    tokio::pin!(stream);
    let mut deltas = 0;
    while let Some(event_res) = stream.next().await {
        let event = event_res?;
        if let InteractionEvent::ContentDelta { .. } = event {
            deltas += 1;
        }
    }
    assert!(deltas > 0);

    println!("3. Testing Search Grounding...");
    let r_search = client.interaction(InteractionInput::Text("What is the weather in NYC?".to_string()))
        .tools(vec![Tool::GoogleSearch])
        .send()
        .await?;
    assert!(!r_search.outputs.is_empty());

    println!("4. Testing File API & Multimodal...");
    let file = client.upload_file("tests/test_file.txt", Some("test_asset".to_string())).await?;
    let r_file = client.interaction(InteractionInput::Parts(vec![
        Part::Text { text: "Summarize this".to_string() },
        Part::Document(chitti::gemini::MediaPart {
            uri: Some(file.uri),
            data: None,
            mime_type: "text/plain".to_string(),
        })
    ])).send().await?;
    assert!(!r_file.outputs.is_empty());
    client.delete_file(&file.name).await?;

    println!("5. Testing Generation Config (temperature)...");
    let config = chitti::gemini::GenerationConfig {
        temperature: Some(0.7),
        ..Default::default()
    };
    let r_temp = client.interaction(InteractionInput::Text("Write a short poem".to_string()))
        .generation_config(config)
        .send()
        .await?;
    assert!(!r_temp.outputs.is_empty());

    println!("DONE!");
    Ok(())
}
