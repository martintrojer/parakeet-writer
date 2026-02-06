use anyhow::Result;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::ChatMessage;
use ollama_rs::generation::parameters::KeepAlive;
use ollama_rs::Ollama;
use std::time::{Duration, Instant};

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
    prompt: String,
}

impl PostProcessor {
    pub fn new(host: &str, port: u16, model: &str, custom_prompt: Option<String>) -> Self {
        // Configure client to handle stale connections after long idle periods
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10)) // Fast fail on dead connections
            .timeout(Duration::from_secs(120)) // Overall request timeout
            .pool_idle_timeout(Duration::from_secs(60)) // Don't keep stale connections
            .pool_max_idle_per_host(0) // Disable connection pooling entirely
            .build()
            .expect("Failed to create HTTP client");

        Self {
            ollama: Ollama::new_with_client(host.to_string(), port, client),
            model: model.to_string(),
            prompt: custom_prompt.unwrap_or_else(|| DEFAULT_PROMPT.to_string()),
        }
    }

    pub async fn process(&self, text: &str) -> Result<String> {
        let total_start = Instant::now();
        let messages = vec![
            ChatMessage::system(self.prompt.clone()),
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

            log::debug!("Sending request to Ollama (attempt {})", attempt + 1);
            let request_start = Instant::now();
            match self.ollama.send_chat_messages(request).await {
                Ok(response) => {
                    log::debug!(
                        "Ollama request succeeded in {:.2}s (total {:.2}s)",
                        request_start.elapsed().as_secs_f32(),
                        total_start.elapsed().as_secs_f32()
                    );
                    return Ok(response.message.content.trim().to_string());
                }
                Err(e) => {
                    log::warn!(
                        "Ollama request failed (attempt {}) after {:.2}s: {}",
                        attempt + 1,
                        request_start.elapsed().as_secs_f32(),
                        e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap().into())
    }
}
