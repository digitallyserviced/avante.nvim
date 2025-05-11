#[cfg(test)]
mod tests {
    use crate::{
        http::HttpClient,
        session::Session,
        RequestBody, RequestOptions,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_http_client_creation() {
        let client = HttpClient::new();
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_http_client_with_options() {
        let options = RequestOptions {
            url: "https://httpbin.org/get".to_string(),
            method: Some("GET".to_string()),
            timeout: Some(30),
            insecure: Some(false),
            ..Default::default()
        };

        let client = HttpClient::new_from_options(&options);
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_session_creation() {
        let session = Session::new();
        
        // Initialize a request
        let request_id = "test_request_123";
        let cancel_flag = session.init_request(request_id);
        
        // Check request was initialized
        let response = session.get_response(request_id);
        assert_eq!(response.request_id, request_id);
        assert!(!response.completed);
        assert!(response.error.is_none());
        
        // Test cancellation
        assert!(!session.should_cancel(request_id));
        session.cancel_request(request_id);
        assert!(session.should_cancel(request_id));
        
        // Test setting response
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        session.set_response(request_id, 200, headers, "Test body");
        
        let response = session.get_response(request_id);
        assert_eq!(response.status, Some(200));
        assert_eq!(response.body, Some("Test body".to_string()));
        
        // Test completion
        session.set_completed(request_id);
        let response = session.get_response(request_id);
        assert!(response.completed);
    }

    #[tokio::test]
    async fn test_stream_handler() {
        let session = Session::new();
        let request_id = "test_stream_123";
        session.init_request(request_id);
        
        let received_data = Arc::new(std::sync::Mutex::new(Vec::new()));
        let received_clone = received_data.clone();
        
        // Set a stream handler
        session.set_stream_handler(request_id, move |data| {
            let mut received = received_clone.lock().unwrap();
            received.push(data);
        });
        
        // Test stream events
        assert!(session.handle_stream_event(request_id, "chunk1"));
        assert!(session.handle_stream_event(request_id, "chunk2"));
        assert!(session.handle_stream_event(request_id, "chunk3"));
        
        let received = received_data.lock().unwrap();
        assert_eq!(received.len(), 3);
        assert_eq!(received[0], "chunk1");
        assert_eq!(received[1], "chunk2");
        assert_eq!(received[2], "chunk3");
    }
}
