use crate::error::AvanteCurlError;
use crate::session::Session;
use crate::util::file;
use crate::RequestOptions;
use anyhow::Result;
use futures_util::stream::StreamExt;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, Method, RequestBuilder, Response, Url,
};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(AvanteCurlError::HttpError)?;

        Ok(Self { client })
    }

    pub fn new_from_options(options: &RequestOptions) -> Result<Self> {
        let mut builder = Client::builder();

        // Set timeout
        if let Some(timeout) = options.timeout {
            builder = builder.timeout(Duration::from_secs(timeout));
        } else {
            builder = builder.timeout(Duration::from_secs(60));
        }

        // Set redirect policy
        if let Some(follow) = options.follow_redirects {
            builder = if follow {
                builder.redirect(reqwest::redirect::Policy::limited(10))
            } else {
                builder.redirect(reqwest::redirect::Policy::none())
            };
        }

        // Set TLS verification
        if let Some(insecure) = options.insecure {
            if insecure {
                builder = builder.danger_accept_invalid_certs(true);
            }
        }

        // Set automatic gzip/deflate/brotli decompression
        if let Some(compressed) = options.compressed {
            builder = builder.gzip(compressed);
            builder = builder.deflate(compressed);
            builder = builder.brotli(compressed);
        } else {
            // Default to automatic decompression
            builder = builder.gzip(true);
            builder = builder.deflate(true);
            builder = builder.brotli(true);
        }

        // Configure proxy if specified
        if let Some(proxy) = &options.proxy {
            let proxy = reqwest::Proxy::all(proxy)
                .map_err(|e| AvanteCurlError::InvalidConfig(format!("Invalid proxy: {}", e)))?;
            builder = builder.proxy(proxy);
        }

        // Configure HTTP version
        if let Some(http_version) = &options.http_version {
            match http_version.as_str() {
                "1.0" => builder = builder.http1_only(),
                "1.1" => builder = builder.http1_only(),
                "2" => builder = builder.http2_prior_knowledge(),
                _ => return Err(AvanteCurlError::InvalidConfig(format!("Unsupported HTTP version: {}", http_version)).into()),
            }
        }

        let client = builder.build()
            .map_err(|e| AvanteCurlError::HttpError(e))?;

        Ok(Self { client })
    }

    pub async fn send_request(&self, options: RequestOptions) -> Result<Response> {
        // Parse the URL
        let url = Url::parse(&options.url)
            .map_err(|e| AvanteCurlError::InvalidConfig(format!("Invalid URL: {}", e)))?;

        // Determine the HTTP method
        let method = match &options.method {
            Some(m) => match m.as_str() {
                "GET" => Method::GET,
                "POST" => Method::POST,
                "PUT" => Method::PUT,
                "DELETE" => Method::DELETE,
                "HEAD" => Method::HEAD,
                "OPTIONS" => Method::OPTIONS,
                "PATCH" => Method::PATCH,
                method => return Err(AvanteCurlError::InvalidConfig(format!("Unsupported HTTP method: {}", method)).into()),
            },
            None => Method::GET,
        };

        // Initialize request builder
        let mut builder = self.client.request(method, url);

        // Add headers
        if let Some(headers) = &options.headers {
            for (key, value) in headers {
                let header_name = HeaderName::from_bytes(key.as_bytes())
                    .map_err(|e| AvanteCurlError::InvalidConfig(format!("Invalid header name: {}", e)))?;

                let header_value = HeaderValue::from_str(value)
                    .map_err(|e| AvanteCurlError::InvalidConfig(format!("Invalid header value: {}", e)))?;

                builder = builder.header(header_name, header_value);
            }
        }

        // Add query parameters
        if let Some(query) = &options.query {
            builder = builder.query(query);
        }

        // Add request body
        if let Some(body) = &options.body {
            builder = match body {
                crate::RequestBody::Raw(raw) => builder.body(raw.clone()),
                crate::RequestBody::Json(json) => builder.json(json),
                crate::RequestBody::File(path) => {
                    let content = file::read_file(path)
                        .map_err(|e| AvanteCurlError::IoError(e))?;
                    builder.body(content)
                }
            };
        }

        // Add form data
        if let Some(form) = &options.form {
            builder = builder.form(form);
        }

        // Add basic auth
        if let Some(auth) = &options.auth {
            builder = builder.basic_auth(&auth.username, Some(&auth.password));
        }

        // Add raw curl arguments
        // This is a simplistic implementation - in a real-world scenario,
        // you'd parse and implement the most common curl options
        if let Some(raw_args) = &options.raw {
            // Implement selected curl arguments
            for i in 0..raw_args.len() {
                if i + 1 >= raw_args.len() {
                    break;
                }

                match raw_args[i].as_str() {
                    "-H" | "--header" => {
                        let header = &raw_args[i + 1];
                        if let Some((name, value)) = header.split_once(':') {
                            let name = name.trim();
                            let value = value.trim();

                            if !name.is_empty() && !value.is_empty() {
                                let header_name = HeaderName::from_bytes(name.as_bytes())
                                    .map_err(|e| AvanteCurlError::InvalidConfig(format!("Invalid header name: {}", e)))?;

                                let header_value = HeaderValue::from_str(value)
                                    .map_err(|e| AvanteCurlError::InvalidConfig(format!("Invalid header value: {}", e)))?;

                                builder = builder.header(header_name, header_value);
                            }
                        }
                    }
                    "-d" | "--data" => {
                        let data = &raw_args[i + 1];
                        builder = builder.body(data.clone());
                    }
                    _ => {}  // Ignore unsupported options
                }
            }
        }

        // Send the request
        let response = builder.send().await.map_err(|e| {
            REQUEST_MANAGER.set_error(request_id, &e.to_string());
            AvanteCurlError::HttpError(e)
        })?;

        // Update request state to Receiving
        REQUEST_MANAGER.set_response(request_id, response.status().as_u16(), HashMap::new(), "");
        Ok(response)
    }

    // Send a request with streaming response, passing chunks to the session
    pub async fn send_stream_request(
        &self,
        options: RequestOptions,
        session: Arc<Session>,
        request_id: String,
        cancel_flag: Arc<AtomicBool>,
    ) -> Result<()> {
        let response = self.send_request(options).await?;

        // Process response headers
        let mut headers_map = std::collections::HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(val_str) = value.to_str() {
                headers_map.insert(key.as_str().to_string(), val_str.to_string());
            }
        }

        // Set initial response with headers
        let status = response.status().as_u16();
        session.set_response(&request_id, status, headers_map.clone(), "");

        // Create stream processor
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let is_sse = content_type.contains("text/event-stream");
        let mut body = response.bytes_stream();
        let mut accumulated_data = String::new();
        let mut buffer = Vec::new();

        while let Some(chunk_result) = body.next().await {
            // Check for cancellation
            if session.should_cancel(&request_id) {
                return Err(AvanteCurlError::Cancelled.into());
            }

            let chunk = chunk_result?;

            if is_sse {
                // Process SSE data
                buffer.extend_from_slice(&chunk);

                // Process complete lines
                let mut start_idx = 0;
                for i in 0..buffer.len() {
                    if i + 1 < buffer.len() && buffer[i] == b'\n' && buffer[i+1] == b'\n' {
                        // Found a complete SSE message
                        if let Ok(data) = String::from_utf8(buffer[start_idx..i].to_vec()) {
                            session.handle_stream_event(&request_id, &data);
                        }
                        start_idx = i + 2;
                    }
                    else if buffer[i] == b'\n' && start_idx < i {
                        // Found a complete line
                        if let Ok(line) = String::from_utf8(buffer[start_idx..i].to_vec()) {
                            let line = line.trim();
                            if line.starts_with("data:") {
                                let data = line[5..].trim();
                                session.handle_stream_event(&request_id, data);
                            }
                        }
                        start_idx = i + 1;
                    }
                }

                // Keep remaining data
                if start_idx < buffer.len() {
                    buffer = buffer[start_idx..].to_vec();
                } else {
                    buffer.clear();
                }
            } else {
                // Regular response - accumulate data
                if let Ok(chunk_str) = String::from_utf8(chunk.to_vec()) {
                    accumulated_data.push_str(&chunk_str);
                    session.handle_stream_event(&request_id, &chunk_str);
                }
            }
        }

        // Complete the request with final data
        session.set_response(&request_id, status, headers_map, &accumulated_data);
        session.set_completed(&request_id);

        Ok(())
    }
}



