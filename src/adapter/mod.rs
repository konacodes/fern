pub mod orchestrator_handler;
pub mod signal;

use std::sync::Arc;

use async_trait::async_trait;

#[async_trait]
pub trait MessagingAdapter: Send + Sync {
    /// Start listening for incoming messages. Calls `handler` for each message.
    /// This should run forever (blocking the task).
    async fn run(&self, handler: Arc<dyn MessageHandler>) -> Result<(), String>;

    /// Send a message to a conversation.
    async fn send_message(&self, conversation_id: &str, text: &str) -> Result<(), String>;
}

#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// Called when a message arrives. Returns the response text.
    async fn handle_message(
        &self,
        sender_id: &str,
        conversation_id: &str,
        text: &str,
    ) -> Result<String, String>;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::{MessageHandler, MessagingAdapter};

    struct MockHandler;

    #[async_trait]
    impl MessageHandler for MockHandler {
        async fn handle_message(
            &self,
            _sender_id: &str,
            _conversation_id: &str,
            _text: &str,
        ) -> Result<String, String> {
            Ok("hello".to_owned())
        }
    }

    struct MockAdapter {
        sent: Arc<Mutex<Vec<(String, String)>>>,
    }

    #[async_trait]
    impl MessagingAdapter for MockAdapter {
        async fn run(&self, handler: Arc<dyn MessageHandler>) -> Result<(), String> {
            let response = handler
                .handle_message("+15550000001", "+15550000002", "ping")
                .await?;
            self.sent
                .lock()
                .map_err(|_| "lock poisoned".to_owned())?
                .push(("+15550000002".to_owned(), response));
            Ok(())
        }

        async fn send_message(&self, conversation_id: &str, text: &str) -> Result<(), String> {
            self.sent
                .lock()
                .map_err(|_| "lock poisoned".to_owned())?
                .push((conversation_id.to_owned(), text.to_owned()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn mock_adapter_sends_and_receives() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let adapter: Arc<dyn MessagingAdapter> = Arc::new(MockAdapter {
            sent: Arc::clone(&sent),
        });
        let handler: Arc<dyn MessageHandler> = Arc::new(MockHandler);

        adapter.run(handler).await.expect("run should succeed");
        let entries = sent.lock().expect("lock should not be poisoned");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], ("+15550000002".to_owned(), "hello".to_owned()));
    }

    #[tokio::test]
    async fn handler_returns_response() {
        let handler: Arc<dyn MessageHandler> = Arc::new(MockHandler);
        let response = handler
            .handle_message("+15550000001", "+15550000002", "ping")
            .await
            .expect("handler should succeed");
        assert_eq!(response, "hello");
    }
}
