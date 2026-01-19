use anyhow::Result;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::ChatMessage;
use ollama_rs::Ollama;

const DEFAULT_PROMPT: &str = "Clean up this voice transcript for use as an AI coding prompt. \
Remove filler words (um, uh, like, you know) and false starts. \
Fix grammar and punctuation. If the speaker corrected themselves, keep only the correction. \
Preserve technical terms and abbreviations exactly as spoken (e.g., API, CLI, async, stdin). \
Preserve the speaker's wording. Only restructure if the original is genuinely unclear. \
Output only the cleaned text.";

pub struct PostProcessor {
    ollama: Ollama,
    model: String,
}

impl PostProcessor {
    pub fn new(host: &str, port: u16, model: &str) -> Self {
        Self {
            ollama: Ollama::new(host.to_string(), port),
            model: model.to_string(),
        }
    }

    pub async fn process(&self, text: &str) -> Result<String> {
        let messages = vec![
            ChatMessage::system(DEFAULT_PROMPT.to_string()),
            ChatMessage::user(text.to_string()),
        ];

        let request = ChatMessageRequest::new(self.model.clone(), messages);
        let response = self.ollama.send_chat_messages(request).await?;

        Ok(response.message.content.trim().to_string())
    }
}
