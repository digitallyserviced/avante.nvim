#[cfg(test)]
mod httpbin_tests {
    use crate::{
        http::HttpClient,
        RequestBody, RequestOptions,
    };
    use std::collections::HashMap;
    use tokio::runtime::Runtime;

    // Helper function to create a tokio runtime for tests
    fn get_runtime() -> Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("test-worker")
            .enable_all()
            .build()
            .expect("Failed to create test runtime")
    }

    #[test]
    fn test_get_request() {
        let rt = get_runtime();

        rt.block_on(async {
            let options = RequestOptions {
                url: "https://httpbin.org/get".to_string(),
                method: Some("GET".to_string()),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            assert_eq!(response.status().as_u16(), 200);

            let body = response.text().await.unwrap();
            let json: serde_json::Value = serde_json::from_str(&body).unwrap();

            assert_eq!(json["url"].as_str().unwrap(), "https://httpbin.org/get");
            assert_eq!(json["args"], serde_json::json!({}));
        });
    }

    #[test]
    fn test_get_with_query_params() {
        let rt = get_runtime();

        rt.block_on(async {
            let mut query_params = HashMap::new();
            query_params.insert("param1".to_string(), "value1".to_string());
            query_params.insert("param2".to_string(), "value2".to_string());

            let options = RequestOptions {
                url: "https://httpbin.org/get".to_string(),
                method: Some("GET".to_string()),
                query: Some(query_params),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            assert_eq!(response.status().as_u16(), 200);

            let body = response.text().await.unwrap();
            let json: serde_json::Value = serde_json::from_str(&body).unwrap();

            assert_eq!(json["args"]["param1"].as_str().unwrap(), "value1");
            assert_eq!(json["args"]["param2"].as_str().unwrap(), "value2");
        });
    }

    #[test]
    fn test_post_with_json_body() {
        let rt = get_runtime();

        rt.block_on(async {
            let json_data = serde_json::json!({
                "name": "test_user",
                "age": 30,
                "tags": ["tag1", "tag2"]
            });

            let options = RequestOptions {
                url: "https://httpbin.org/post".to_string(),
                method: Some("POST".to_string()),
                body: Some(RequestBody::Json(json_data.clone())),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            assert_eq!(response.status().as_u16(), 200);

            let body = response.text().await.unwrap();
            let json_response: serde_json::Value = serde_json::from_str(&body).unwrap();

            assert_eq!(json_response["url"].as_str().unwrap(), "https://httpbin.org/post");

            let json_body: serde_json::Value = serde_json::from_str(json_response["data"].as_str().unwrap()).unwrap();
            assert_eq!(json_body["name"].as_str().unwrap(), "test_user");
            assert_eq!(json_body["age"].as_i64().unwrap(), 30);
            assert_eq!(json_body["tags"][0].as_str().unwrap(), "tag1");
            assert_eq!(json_body["tags"][1].as_str().unwrap(), "tag2");
        });
    }

    #[test]
    fn test_post_with_form_data() {
        let rt = get_runtime();

        rt.block_on(async {
            let mut form_data = HashMap::new();
            form_data.insert("field1".to_string(), "value1".to_string());
            form_data.insert("field2".to_string(), "value2".to_string());

            let options = RequestOptions {
                url: "https://httpbin.org/post".to_string(),
                method: Some("POST".to_string()),
                form: Some(form_data),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            assert_eq!(response.status().as_u16(), 200);

            let body = response.text().await.unwrap();
            let json: serde_json::Value = serde_json::from_str(&body).unwrap();

            assert_eq!(json["form"]["field1"].as_str().unwrap(), "value1");
            assert_eq!(json["form"]["field2"].as_str().unwrap(), "value2");
        });
    }

    #[test]
    fn test_put_request() {
        let rt = get_runtime();

        rt.block_on(async {
            let json_data = serde_json::json!({
                "updated": true,
                "id": 123
            });

            let options = RequestOptions {
                url: "https://httpbin.org/put".to_string(),
                method: Some("PUT".to_string()),
                body: Some(RequestBody::Json(json_data)),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            assert_eq!(response.status().as_u16(), 200);

            let body = response.text().await.unwrap();
            let json_response: serde_json::Value = serde_json::from_str(&body).unwrap();

            let json_body: serde_json::Value = serde_json::from_str(json_response["data"].as_str().unwrap()).unwrap();
            assert_eq!(json_body["updated"].as_bool().unwrap(), true);
            assert_eq!(json_body["id"].as_i64().unwrap(), 123);
        });
    }

    #[test]
    fn test_delete_request() {
        let rt = get_runtime();

        rt.block_on(async {
            let options = RequestOptions {
                url: "https://httpbin.org/delete".to_string(),
                method: Some("DELETE".to_string()),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            assert_eq!(response.status().as_u16(), 200);

            let body = response.text().await.unwrap();
            let json: serde_json::Value = serde_json::from_str(&body).unwrap();

            assert_eq!(json["url"].as_str().unwrap(), "https://httpbin.org/delete");
        });
    }

    #[test]
    fn test_headers() {
        let rt = get_runtime();

        rt.block_on(async {
            let mut headers = HashMap::new();
            headers.insert("X-Custom-Header".to_string(), "test-value".to_string());
            headers.insert("User-Agent".to_string(), "avante-curl-test".to_string());

            let options = RequestOptions {
                url: "https://httpbin.org/headers".to_string(),
                method: Some("GET".to_string()),
                headers: Some(headers),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            assert_eq!(response.status().as_u16(), 200);

            let body = response.text().await.unwrap();
            let json: serde_json::Value = serde_json::from_str(&body).unwrap();

            assert_eq!(json["headers"]["X-Custom-Header"].as_str().unwrap(), "test-value");
            assert_eq!(json["headers"]["User-Agent"].as_str().unwrap(), "avante-curl-test");
        });
    }

    #[test]
    fn test_basic_auth() {
        let rt = get_runtime();

        rt.block_on(async {
            let auth = crate::AuthInfo {
                username: "user".to_string(),
                password: "passwd".to_string(),
            };

            let options = RequestOptions {
                url: "https://httpbin.org/basic-auth/user/passwd".to_string(),
                method: Some("GET".to_string()),
                auth: Some(auth),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            assert_eq!(response.status().as_u16(), 200);

            let body = response.text().await.unwrap();
            let json: serde_json::Value = serde_json::from_str(&body).unwrap();

            assert_eq!(json["authenticated"].as_bool().unwrap(), true);
            assert_eq!(json["user"].as_str().unwrap(), "user");
        });
    }

    #[test]
    fn test_follow_redirects() {
        let rt = get_runtime();

        rt.block_on(async {
            let options = RequestOptions {
                url: "https://httpbin.org/redirect/2".to_string(),
                method: Some("GET".to_string()),
                follow_redirects: Some(true),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            // Should follow redirects and eventually get 200
            assert_eq!(response.status().as_u16(), 200);
        });
    }

    #[test]
    fn test_no_follow_redirects() {
        let rt = get_runtime();

        rt.block_on(async {
            let options = RequestOptions {
                url: "https://httpbin.org/redirect/2".to_string(),
                method: Some("GET".to_string()),
                follow_redirects: Some(false),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            // Should not follow redirect and get 302
            assert_eq!(response.status().as_u16(), 302);
        });
    }

    #[test]
    fn test_timeout() {
        let rt = get_runtime();

        rt.block_on(async {
            let options = RequestOptions {
                // This endpoint delays the response by 5 seconds
                url: "https://httpbin.org/delay/5".to_string(),
                method: Some("GET".to_string()),
                // Set timeout to 1 second, which should cause the request to time out
                timeout: Some(1),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let result = client.send_request(options).await;

            // Request should fail, but the error might be different depending on the environment
            // (timeout, connection reset, etc.)
            assert!(result.is_err());
            println!("Expected timeout error: {}", result.unwrap_err());
        });
    }

    #[test]
    fn test_gzip_response() {
        let rt = get_runtime();

        rt.block_on(async {
            let mut headers = HashMap::new();
            headers.insert("Accept-Encoding".to_string(), "gzip".to_string());

            let options = RequestOptions {
                url: "https://httpbin.org/gzip".to_string(),
                method: Some("GET".to_string()),
                headers: Some(headers),
                compressed: Some(true),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            assert_eq!(response.status().as_u16(), 200);

            let body = response.text().await.unwrap();

            // Print the response body to debug
            println!("Received response body: {}", body);

            // Parse JSON with better error handling
            let json: serde_json::Value = match serde_json::from_str(&body) {
                Ok(json) => json,
                Err(e) => {
                    println!("JSON parsing error: {}", e);
                    println!("Response body: {}", body);
                    panic!("Failed to parse response as JSON");
                }
            };

            // Verify that we got a response with gzip info
            assert!(json.is_object());
            assert!(json.get("gzipped").is_some());
            assert_eq!(json["gzipped"].as_bool().unwrap_or(false), true);
        });
    }

    #[test]
    fn test_raw_body() {
        let rt = get_runtime();

        rt.block_on(async {
            let raw_data = "This is raw text data for testing";

            let options = RequestOptions {
                url: "https://httpbin.org/post".to_string(),
                method: Some("POST".to_string()),
                body: Some(RequestBody::Raw(raw_data.to_string())),
                ..Default::default()
            };

            let client = HttpClient::new_from_options(&options).unwrap();
            let response = client.send_request(options).await.unwrap();

            assert_eq!(response.status().as_u16(), 200);

            let body = response.text().await.unwrap();
            let json: serde_json::Value = serde_json::from_str(&body).unwrap();

            assert_eq!(json["data"].as_str().unwrap(), raw_data);
        });
    }

    #[test]
    fn test_status_codes() {
        let rt = get_runtime();

        // Test a few different status codes
        let status_codes = [200, 404, 418, 500];

        for code in status_codes.iter() {
            rt.block_on(async {
                let options = RequestOptions {
                    url: format!("https://httpbin.org/status/{}", code),
                    method: Some("GET".to_string()),
                    ..Default::default()
                };

                let client = HttpClient::new_from_options(&options).unwrap();
                let response = client.send_request(options).await.unwrap();

                assert_eq!(response.status().as_u16(), *code);
            });
        }
    }
}


