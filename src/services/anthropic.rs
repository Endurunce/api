use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<Message>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    text: String,
}

pub async fn complete(
    http: &reqwest::Client,
    config: &Config,
    system: Option<&str>,
    messages: Vec<Message>,
    max_tokens: u32,
) -> Result<String> {
    let api_key = config.anthropic_api_key.as_deref()
        .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

    let body = AnthropicRequest {
        model: config.anthropic_model.clone(),
        max_tokens,
        system: system.map(String::from),
        messages,
    };

    let resp = http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Anthropic API error: {}", text));
    }

    let result: AnthropicResponse = resp.json().await?;
    Ok(result.content.into_iter().map(|b| b.text).collect::<Vec<_>>().join(""))
}
