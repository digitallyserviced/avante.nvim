use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::fmt;

// Request state enum to track current status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestState {
    Init,       // Request is initialized but not started
    Sending,    // Request is being sent
    Receiving,  // Request is receiving data
    Complete,   // Request completed successfully
    Error,      // Request encountered an error
    Timeout,    // Request timed out
    Cancelled,  // Request was cancelled
    Idle,       // Request is idle (no recent activity)
    Acknowledged, // Request completion was acknowledged by client
}

impl fmt::Display for RequestState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequestState::Init => write!(f, "init"),
            RequestState::Sending => write!(f, "sending"),
            RequestState::Receiving => write!(f, "receiving"),
            RequestState::Complete => write!(f, "complete"),
            RequestState::Error => write!(f, "error"),
            RequestState::Timeout => write!(f, "timeout"),
            RequestState::Cancelled => write!(f, "cancelled"),
            RequestState::Idle => write!(f, "idle"),
            RequestState::Acknowledged => write!(f, "acknowledged"),
        }
    }
}

// Response information stored per request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestInfo {
    pub request_id: String,
    pub state: RequestState,
    pub status: Option<u16>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    pub error: Option<String>,
    pub last_polled: u64,    // Timestamp of last poll
    pub created_at: u64,     // Timestamp of creation
    pub updated_at: u64,     // Timestamp of last update
}

// Callback handlers for request events
pub struct CallbackHandlers {
    pub on_chunk: Option<Arc<Mutex<Box<dyn Fn(&str) + Send + 'static>>>>,
    pub on_complete: Option<Arc<Mutex<Box<dyn Fn(&RequestInfo) + Send + 'static>>>>,
    pub on_error: Option<Arc<Mutex<Box<dyn Fn(&str) + Send + 'static>>>>,
}

// RequestManager keeps track of request states
pub struct RequestManager {
    requests: DashMap<String, Arc<RwLock<RequestInfo>>>,
    callbacks: DashMap<String, CallbackHandlers>,
    cancellations: DashMap<String, Arc<AtomicBool>>,
    idle_timeout: u64,       // Seconds after which an unpolled request is considered idle
    cleanup_interval: u64,   // Seconds between cleanup operations
    last_cleanup: Arc<AtomicU64>,  // Timestamp of last cleanup
}

// Session class to handle requests for a specific client
pub struct Session {
    request_manager: RequestManager,
}

impl Session {
    pub fn new() -> Self {
        Self {
            request_manager: RequestManager::new(),
        }
    }

    pub fn with_config(idle_timeout: u64, cleanup_interval: u64) -> Self {
        Self {
            request_manager: RequestManager::with_config(idle_timeout, cleanup_interval),
        }
    }

    pub fn init_request(&self, request_id: &str) -> Result<Arc<AtomicBool>, String> {
        self.request_manager.init_request(request_id)
    }

    pub fn get_response(&self, request_id: &str) -> RequestInfo {
        match self.request_manager.poll_request(request_id) {
            Some(info) => info,
            None => RequestInfo {
                request_id: request_id.to_string(),
                state: RequestState::Error,
                status: None,
                headers: None,
                body: None,
                error: Some(format!("Request '{}' not found", request_id)),
                last_polled: Self::timestamp_now(),
                created_at: Self::timestamp_now(),
                updated_at: Self::timestamp_now(),
            },
        }
    }

    pub fn set_response(&self, request_id: &str, status: u16, headers: HashMap<String, String>, body: &str) {
        self.request_manager.set_response(request_id, status, headers, body);
    }

    pub fn set_completed(&self, request_id: &str) {
        self.request_manager.set_completed(request_id);
    }

    pub fn set_error(&self, request_id: &str, error: &str) {
        self.request_manager.set_error(request_id, error);
    }

    pub fn handle_stream_event(&self, request_id: &str, data: &str) {
        self.request_manager.handle_chunk(request_id, data);
    }

    pub fn cancel_request(&self, request_id: &str) {
        self.request_manager.cancel_request(request_id);
    }

    pub fn should_cancel(&self, request_id: &str) -> bool {
        self.request_manager.should_cancel(request_id)
    }

    pub fn set_callbacks(&self, request_id: &str,
                         on_chunk: Option<Box<dyn Fn(&str) + Send + 'static>>,
                         on_complete: Option<Box<dyn Fn(&RequestInfo) + Send + 'static>>,
                         on_error: Option<Box<dyn Fn(&str) + Send + 'static>>) {
        self.request_manager.set_callbacks(request_id, on_chunk, on_complete, on_error);
    }

    // Helper to get current timestamp
    fn timestamp_now() -> u64 {
        RequestManager::timestamp_now()
    }
}

impl RequestManager {
    pub fn new() -> Self {
        Self {
            requests: DashMap::new(),
            callbacks: DashMap::new(),
            cancellations: DashMap::new(),
            idle_timeout: 3600,       // Default: 1 hour
            cleanup_interval: 300,    // Default: 5 minutes
            last_cleanup: Arc::new(AtomicU64::new(Self::timestamp_now())),
        }
    }

    pub fn init_request(&self, request_id: &str) -> Result<Arc<AtomicBool>, String> {
        let now = Self::timestamp_now();
        let cancel_flag = Arc::new(AtomicBool::new(false));

        if let Some(existing) = self.requests.get(request_id) {
            let mut req = existing.write().unwrap();
            match req.state {
                RequestState::Complete | RequestState::Error | RequestState::Timeout | RequestState::Cancelled | RequestState::Idle => {
                    req.state = RequestState::Init;
                    req.status = None;
                    req.headers = None;
                    req.body = None;
                    req.error = None;
                    req.last_polled = now;
                    req.updated_at = now;

                    self.cancellations.insert(request_id.to_string(), cancel_flag.clone());

                    Ok(cancel_flag)
                },
                _ => Err(format!("Request '{}' is already in progress with state: {}", request_id, req.state))
            }
        } else {
            let request_info = RequestInfo {
                request_id: request_id.to_string(),
                state: RequestState::Init,
                status: None,
                headers: None,
                body: None,
                error: None,
                last_polled: now,
                created_at: now,
                updated_at: now,
            };

            self.requests.insert(request_id.to_string(), Arc::new(RwLock::new(request_info)));
            self.cancellations.insert(request_id.to_string(), cancel_flag.clone());

            Ok(cancel_flag)
        }
    }

    pub fn poll_request(&self, request_id: &str) -> Option<RequestInfo> {
        let now = Self::timestamp_now();
        self.try_cleanup(now);

        if let Some(req_lock) = self.requests.get(request_id) {
            let mut req = req_lock.write().unwrap();
            req.last_polled = now;

            if req.state == RequestState::Sending || req.state == RequestState::Receiving {
                let time_since_update = now - req.updated_at;
                if time_since_update > 30 {
                    req.state = RequestState::Timeout;
                    req.error = Some("Request timed out".to_string());
                }
            }

            return Some(req.clone());
        }

        None
    }

    pub fn set_response(&self, request_id: &str, status: u16, headers: HashMap<String, String>, body: &str) {
        if let Some(req_lock) = self.requests.get(request_id) {
            let mut req = req_lock.write().unwrap();
            req.status = Some(status);
            req.headers = Some(headers);
            req.body = Some(body.to_string());
            req.updated_at = Self::timestamp_now();
        }
    }

    pub fn set_completed(&self, request_id: &str) {
        let req_info = {
            if let Some(req_lock) = self.requests.get(request_id) {
                let mut req = req_lock.write().unwrap();
                req.state = RequestState::Complete;
                req.updated_at = Self::timestamp_now();
                req.clone()
            } else {
                return;
            }
        };

        if let Some(callbacks) = self.callbacks.get(request_id) {
            if let Some(on_complete) = &callbacks.on_complete {
                if let Ok(handler) = on_complete.lock() {
                    handler(&req_info);
                }
            }
        }
    }

    pub fn set_error(&self, request_id: &str, error: &str) {
        if let Some(req_lock) = self.requests.get(request_id) {
            let mut req = req_lock.write().unwrap();
            req.state = RequestState::Error;
            req.error = Some(error.to_string());
            req.updated_at = Self::timestamp_now();
        }

        if let Some(callbacks) = self.callbacks.get(request_id) {
            if let Some(on_error) = &callbacks.on_error {
                if let Ok(handler) = on_error.lock() {
                    handler(error);
                }
            }
        }
    }
}

impl fmt::Debug for RequestManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestManager")
            .field("requests", &self.requests)
            .field("idle_timeout", &self.idle_timeout)
            .field("cleanup_interval", &self.cleanup_interval)
            .finish()
    }
}

impl RequestManager {
    pub fn new() -> Self {
        Self {
            requests: DashMap::new(),
            callbacks: DashMap::new(),
            cancellations: DashMap::new(),
            idle_timeout: 3600,       // Default: 1 hour
            cleanup_interval: 300,    // Default: 5 minutes
            last_cleanup: Arc::new(AtomicU64::new(Self::timestamp_now())),
        }
    }

    pub fn with_config(idle_timeout: u64, cleanup_interval: u64) -> Self {
        Self {
            requests: DashMap::new(),
            callbacks: DashMap::new(),
            cancellations: DashMap::new(),
            idle_timeout,
            cleanup_interval,
            last_cleanup: Arc::new(AtomicU64::new(Self::timestamp_now())),
        }
    }

    // Get current timestamp in seconds
    fn timestamp_now() -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0));
        now.as_secs()
    }

    // Initialize a request with client-provided ID
    pub fn init_request(&self, request_id: &str) -> Result<Arc<AtomicBool>, String> {
        let now = Self::timestamp_now();
        let cancel_flag = Arc::new(AtomicBool::new(false));

        // Check if request already exists
        if let Some(existing) = self.requests.get(request_id) {
            // Request exists, check if it can be reset
            let mut req = existing.write().unwrap();
            match req.state {
                RequestState::Complete |
                RequestState::Error |
                RequestState::Timeout |
                RequestState::Cancelled |
                RequestState::Idle => {
                    // Reset the request state
                    req.state = RequestState::Init;
                    req.status = None;
                    req.headers = None;
                    req.body = None;
                    req.error = None;
                    req.last_polled = now;
                    req.updated_at = now;

                    // Reset cancellation flag
                    self.cancellations.insert(request_id.to_string(), cancel_flag.clone());

                    Ok(cancel_flag)
                },
                _ => {
                    // Request is still in progress
                    Err(format!("Request '{}' is already in progress with state: {}", request_id, req.state))
                }
            }
        } else {
            // Create a new request
            let request_info = RequestInfo {
                request_id: request_id.to_string(),
                state: RequestState::Init,
                status: None,
                headers: None,
                body: None,
                error: None,
                last_polled: now,
                created_at: now,
                updated_at: now,
            };

            self.requests.insert(request_id.to_string(), Arc::new(RwLock::new(request_info)));
            self.cancellations.insert(request_id.to_string(), cancel_flag.clone());

            Ok(cancel_flag)
        }
    }

    // Set callbacks for a request
    pub fn set_callbacks(&self, request_id: &str,
                         on_chunk: Option<Box<dyn Fn(&str) + Send + 'static>>,
                         on_complete: Option<Box<dyn Fn(&RequestInfo) + Send + 'static>>,
                         on_error: Option<Box<dyn Fn(&str) + Send + 'static>>) {

        let handlers = CallbackHandlers {
            on_chunk: on_chunk.map(|h| Arc::new(Mutex::new(h))),
            on_complete: on_complete.map(|h| Arc::new(Mutex::new(h))),
            on_error: on_error.map(|h| Arc::new(Mutex::new(h))),
        };

        self.callbacks.insert(request_id.to_string(), handlers);
    }

    // Process a chunk of data from the response
    pub fn handle_chunk(&self, request_id: &str, data: &str) -> bool {
        // Update request state
        if let Some(req_lock) = self.requests.get(request_id) {
            let mut req = req_lock.write().unwrap();
            req.state = RequestState::Receiving;
            req.updated_at = Self::timestamp_now();

            // Append to body if it exists
            if let Some(body) = &mut req.body {
                body.push_str(data);
            } else {
                req.body = Some(data.to_string());
            }
        } else {
            return false;
        }

        // Call the on_chunk callback if it exists
        if let Some(callbacks) = self.callbacks.get(request_id) {
            if let Some(on_chunk) = &callbacks.on_chunk {
                if let Ok(handler) = on_chunk.lock() {
                    handler(data);
                    return true;
                }
            }
        }

        false
    }

    // Set the response for a request
    pub fn set_response(&self, request_id: &str, status: u16, headers: HashMap<String, String>, body: &str) {
        if let Some(req_lock) = self.requests.get(request_id) {
            let mut req = req_lock.write().unwrap();
            req.status = Some(status);
            req.headers = Some(headers);
            req.body = Some(body.to_string());
            req.updated_at = Self::timestamp_now();
        }
    }

    // Mark a request as complete and trigger callbacks
    pub fn set_completed(&self, request_id: &str) {
        let req_info = {
            if let Some(req_lock) = self.requests.get(request_id) {
                let mut req = req_lock.write().unwrap();
                req.state = RequestState::Complete;
                req.updated_at = Self::timestamp_now();
                req.clone()
            } else {
                return;
            }
        };

        // Call the on_complete callback if it exists
        if let Some(callbacks) = self.callbacks.get(request_id) {
            if let Some(on_complete) = &callbacks.on_complete {
                if let Ok(handler) = on_complete.lock() {
                    handler(&req_info);
                }
            }
        }
    }

    // Set an error for a request and trigger callbacks
    pub fn set_error(&self, request_id: &str, error: &str) {
        // Update request state
        if let Some(req_lock) = self.requests.get(request_id) {
            let mut req = req_lock.write().unwrap();
            req.state = RequestState::Error;
            req.error = Some(error.to_string());
            req.updated_at = Self::timestamp_now();
        }

        // Call the on_error callback if it exists
        if let Some(callbacks) = self.callbacks.get(request_id) {
            if let Some(on_error) = &callbacks.on_error {
                if let Ok(handler) = on_error.lock() {
                    handler(error);
                }
            }
        }
    }

    // Acknowledge that a client has processed a completed request
    pub fn acknowledge_request(&self, request_id: &str) -> bool {
        if let Some(req_lock) = self.requests.get(request_id) {
            let mut req = req_lock.write().unwrap();

            // Only acknowledge if in terminal state
            match req.state {
                RequestState::Complete |
                RequestState::Error |
                RequestState::Timeout |
                RequestState::Cancelled => {
                    req.state = RequestState::Acknowledged;
                    req.updated_at = Self::timestamp_now();
                    return true;
                },
                _ => return false,
            }
        }
        false
    }

    // Poll a request and update its status
    pub fn poll_request(&self, request_id: &str) -> Option<RequestInfo> {
        let now = Self::timestamp_now();

        // Try to run cleanup if it's time
        self.try_cleanup(now);

        if let Some(req_lock) = self.requests.get(request_id) {
            let mut req = req_lock.write().unwrap();
            req.last_polled = now;

            // Check for timeouts
            if req.state == RequestState::Sending || req.state == RequestState::Receiving {
                let time_since_update = now - req.updated_at;
                // If no updates for 30 seconds, consider it a timeout
                if time_since_update > 30 {
                    req.state = RequestState::Timeout;
                    req.error = Some("Request timed out".to_string());
                }
            }

            return Some(req.clone());
        }

        None
    }

    // Check if a request should be cancelled
    pub fn should_cancel(&self, request_id: &str) -> bool {
        match self.cancellations.get(request_id) {
            Some(flag) => flag.load(Ordering::SeqCst),
            None => false,
        }
    }

    // Cancel a request
    pub fn cancel_request(&self, request_id: &str) {
        // Set cancel flag
        if let Some(flag) = self.cancellations.get(request_id) {
            flag.store(true, Ordering::SeqCst);
        }

        // Update request state
        if let Some(req_lock) = self.requests.get(request_id) {
            let mut req = req_lock.write().unwrap();
            req.state = RequestState::Cancelled;
            req.error = Some("Request was cancelled".to_string());
            req.updated_at = Self::timestamp_now();
        }
    }

    // Try to run the cleanup procedure if enough time has passed
    fn try_cleanup(&self, now: u64) {
        let last_cleanup = self.last_cleanup.load(Ordering::Relaxed);
        if now - last_cleanup > self.cleanup_interval {
            if self.last_cleanup.compare_exchange(
                last_cleanup,
                now,
                Ordering::SeqCst,
                Ordering::Relaxed
            ).is_ok() {
                self.cleanup_idle_requests(now);
            }
        }
    }

    // Clean up idle and acknowledged requests
    fn cleanup_idle_requests(&self, now: u64) {
        let to_remove: Vec<String> = self.requests
            .iter()
            .filter_map(|entry| {
                let req = entry.value().read().unwrap();

                // Remove if acknowledged
                if req.state == RequestState::Acknowledged {
                    return Some(req.request_id.clone());
                }

                // Check if idle
                let time_since_poll = now - req.last_polled;
                if time_since_poll > self.idle_timeout {
                    // Mark as idle if not already in a terminal state
                    if !matches!(req.state,
                        RequestState::Complete |
                        RequestState::Error |
                        RequestState::Timeout |
                        RequestState::Cancelled |
                        RequestState::Idle |
                        RequestState::Acknowledged) {

                        // We'll update it to idle without removing
                        return None;
                    }

                    // Already in terminal state and idle - remove it
                    return Some(req.request_id.clone());
                }

                None
            })
            .collect();

        // Mark idle requests
        for entry in self.requests.iter() {
            let mut req = entry.value().write().unwrap();
            let time_since_poll = now - req.last_polled;

            if time_since_poll > self.idle_timeout &&
               !matches!(req.state,
                   RequestState::Complete |
                   RequestState::Error |
                   RequestState::Timeout |
                   RequestState::Cancelled |
                   RequestState::Idle |
                   RequestState::Acknowledged) {

                req.state = RequestState::Idle;
                req.updated_at = now;
            }
        }

        // Remove requests
        for id in to_remove {
            self.requests.remove(&id);
            self.callbacks.remove(&id);
            self.cancellations.remove(&id);
        }
    }
}




