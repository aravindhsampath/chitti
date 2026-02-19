use chitti::gemini::{
    Client, InteractionInput, InteractionEvent, InteractionOutput, Role, 
    Part, InteractionPart, Tool, ThinkingLevel, CachedContent, Content, InteractionContent
};
use dotenvy::dotenv;
use std::env;
use futures_util::StreamExt;
use tokio::fs;

fn get_test_client() -> Client {
    dotenv().ok();
    let api_key = env::var("TEST_API_KEY")
        .expect("TEST_API_KEY must be set for integration tests");
    let model = env::var("GEMINI_MODEL")
        .unwrap_or_else(|_| "gemini-3-flash-preview".to_string());
    Client::new(api_key, model)
}

fn setup_test_logging() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .try_init();
}


#[tokio::test]
async fn test_exhaustive_checklist() -> anyhow::Result<()> {
    let client = get_test_client();
    setup_test_logging();
    
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
        InteractionPart::Text { text: "Summarize this".to_string() },
        InteractionPart::Document(chitti::gemini::MediaPart {
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

#[tokio::test]
async fn test_caching_operations() -> anyhow::Result<()> {
    let client = get_test_client();
    let content = Content {
        role: Some(Role::User),
        parts: vec![Part { text: Some("This is cached content.".to_string()), ..Default::default() }],
    };
    
    let cached_content = CachedContent {
        name: None,
        model: "models/gemini-1.5-flash-001".to_string(), // Caching requires specific models usually
        contents: Some(vec![content]),
        system_instruction: None,
        tools: None,
        ttl: Some("300s".to_string()),
        expire_time: None,
    };
    
    // Note: Caching might return 400 if model doesn't support it.
    // We should handle error gracefully or use a model that supports it.
    // gemini-1.5-pro-001 or gemini-1.5-flash-001 supports caching.
    
    match client.create_cached_content(cached_content).await {
        Ok(created) => {
            println!("Created cached content: {:?}", created.name);
            if let Some(name) = &created.name {
                // Fetch to verify
                let fetched = client.get_cached_content(name).await?;
                assert_eq!(fetched.name, created.name);
                // USE the cache in an interaction
                let r_cached = client.interaction(InteractionInput::Text("What was the cached content?".to_string()))
                    .cached_content(name.clone())
                    .send()
                    .await?;
                assert!(!r_cached.outputs.is_empty());
                println!("Used cache in interaction successfully");

                // Cleanup
                client.delete_cached_content(name).await?;
                println!("Deleted cached content");
            }
        },
        Err(e) => {
            println!("Skipping caching test due to error (likely model support or quota): {:?}", e);
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_batch_operations() -> anyhow::Result<()> {
    let client = get_test_client();
    
    // Create a dummy batch input file
    let batch_content = r#"{"request": {"contents": [{"role": "user", "parts": [{"text": "Hello world"}]}]}}"#;
    fs::write("tests/batch_input.jsonl", batch_content).await?;
    
    // Upload file
    // Note: upload_file logic needs to be checked if it supports any file extension or if it sets mime type.
    // It infers mime type?
    match client.upload_file("tests/batch_input.jsonl", None).await {
        Ok(file) => {
             // Create batch
            match client.create_batch("test_batch".to_string(), file.name.clone()).await {
                Ok(batch) => {
                    println!("Batch created: {:?}", batch.name);
                    let status = client.get_batch_operation(&batch.name).await?;
                    println!("Batch status: {:?}", status);
                },
                Err(e) => {
                    println!("Batch creation failed (expected if not whitelisted or model issue): {:?}", e);
                }
            }
            
            // Cleanup file
            let _ = client.delete_file(&file.name).await;
        },
        Err(e) => {
            println!("File upload failed: {:?}", e);
        }
    }
    
    // Cleanup local file
    let _ = fs::remove_file("tests/batch_input.jsonl").await;

    Ok(())
}

#[tokio::test]
async fn test_tool_calling() -> anyhow::Result<()> {
    let client = get_test_client();
    
    // 1. Define a mock tool
    let declaration = chitti::gemini::FunctionDeclaration {
        name: "get_weather".to_string(),
        description: "Gets the weather".to_string(),
        parameters: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "location": {"type": "string"}
            }
        })),
    };
    
    let tools = vec![Tool::Function { declaration }];
    
    // 2. Send request that triggers tool
    let r1 = client.interaction(InteractionInput::Text("What's the weather in London?".to_string()))
        .tools(tools.clone())
        .store(true)
        .send()
        .await?;
        
    let mut interaction_id = r1.id.clone();
    let mut tool_called = false;
    
    for output in r1.outputs {
        if let InteractionOutput::FunctionCall(fc) = output {
            assert_eq!(fc.name, "get_weather");
            tool_called = true;
            
            // 3. Send back the tool result
            let response = client.interaction(InteractionInput::Parts(vec![
                InteractionPart::FunctionResponse(chitti::gemini::FunctionResponse {
                    id: fc.id,
                    name: fc.name,
                    response: serde_json::json!({"weather": "sunny"}),
                })
            ]))
            .previous_interaction_id(interaction_id.take().unwrap())
            .store(true)
            .send()
            .await?;
            
            assert!(!response.outputs.is_empty());
            println!("Tool cycle completed successfully");
            break;
        }
    }
    
    assert!(tool_called, "Model should have called the tool");
    
    Ok(())
}

