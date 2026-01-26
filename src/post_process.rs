use anyhow::Result;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::ChatMessage;
use ollama_rs::generation::parameters::KeepAlive;
use ollama_rs::Ollama;
use std::time::Duration;

const DEFAULT_PROMPT: &str = "Clean up this voice transcript for use as an AI coding prompt. \
Remove filler words (um, uh, like, you know) and false starts. \
Fix grammar and punctuation. If the speaker corrected themselves, keep only the correction. \
Replace spoken punctuation and symbol names with their actual characters \
(e.g., \"comma\" → ,, \"open paren\" → (, \"hash\" → #, \"greater than\" → >). \
Preserve technical terms and abbreviations exactly as spoken (e.g., API, CLI, async, stdin). \
Preserve the speaker's wording. Only restructure if the original is genuinely unclear. \
Output only the cleaned text.";

pub struct PostProcessor {
    ollama: Ollama,
    model: String,
}

impl PostProcessor {
    pub fn new(host: &str, port: u16, model: &str) -> Self {
        // Use a longer timeout to handle cases where the model needs to reload
        // after long idle periods (default reqwest timeout may be too short)
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300)) // 5 minute timeout for slow model loads
            .build()
            .expect("Failed to create HTTP client");

        Self {
            ollama: Ollama::new_with_client(host.to_string(), port, client),
            model: model.to_string(),
        }
    }

    pub async fn process(&self, text: &str) -> Result<String> {
        let messages = vec![
            ChatMessage::system(DEFAULT_PROMPT.to_string()),
            ChatMessage::user(text.to_string()),
        ];

        // Retry logic for stale connections after long idle periods (days)
        let mut last_error = None;
        for attempt in 0..3 {
            if attempt > 0 {
                log::info!("Retrying Ollama request (attempt {})", attempt + 1);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let request = ChatMessageRequest::new(self.model.clone(), messages.clone())
                .think(false)
                .keep_alive(KeepAlive::Indefinitely);

            match self.ollama.send_chat_messages(request).await {
                Ok(response) => return Ok(response.message.content.trim().to_string()),
                Err(e) => {
                    log::warn!("Ollama request failed: {}", e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap().into())
    }
}
