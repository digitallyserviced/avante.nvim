use anyhow::Result;
use dashmap::DashMap;
use mlua::{prelude::*, Lua};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;
use uuid::Uuid;

mod error;
mod http;
mod httpbin_tests;
mod session;
mod util;

use http::HttpClient;
use session::{RequestManager, Session};

// Global state management
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("avante-curl-worker")
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime")
});

static SESSIONS: Lazy<DashMap<String, Arc<Session>>> = Lazy::new(|| {
    DashMap::new()
});

pub fn request_manager_status() -> String {
    let sessions_count = SESSIONS.len();
    format!("Active sessions: {}\n", sessions_count)
}

// Request types
#[derive(Debug, Serialize, Deserialize)]
struct RequestOptions {
    url: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<RequestBody>,
    query: Option<HashMap<String, String>>,
    form: Option<HashMap<String, String>>,
    auth: Option<AuthInfo>,
    timeout: Option<u64>,
    dump: Option<Vec<String>>,
    output: Option<String>,
    follow_redirects: Option<bool>,
    insecure: Option<bool>,
    proxy: Option<String>,
    compressed: Option<bool>,
    raw: Option<Vec<String>>,
    http_version: Option<String>,
}

impl FromLua for RequestOptions {
    fn from_lua(value: LuaValue, _lua: &Lua) -> LuaResult<Self> {
        if let LuaValue::Table(table) = value {
            let mut options = RequestOptions::default();
            for pair in table.pairs::<String, LuaValue>() {
                let (key, value) = pair?;
                match key.as_str() {
                    "url" => options.url = value.to_string().unwrap_or_default(),
                    "method" => options.method = Some(value.to_string().unwrap_or_default()),
                    "headers" => {
                        if let LuaValue::Table(headers_table) = value {
                            options.headers = Some(
                                headers_table
                                    .pairs::<String, String>()
                                    .map(|pair| pair.unwrap())
                                    .collect(),
                            );
                        }
                    }
                    // Handle other fields similarly...
                    _ => {}
                }
            }
            Ok(options)
        } else {
            Err(LuaError::FromLuaConversionError {
                from: "LuaValue",
                to: "RequestOptions".to_string(),
                message: Some("Expected a table".to_string()),
            })
        }
    }
}

impl Default for RequestOptions {
    fn default() -> Self {
        RequestOptions {
            url: String::new(),
            method: None,
            headers: None,
            body: None,
            query: None,
            form: None,
            auth: None,
            timeout: None,
            dump: None,
            output: None,
            follow_redirects: None,
            insecure: None,
            proxy: None,
            compressed: None,
            raw: None,
            http_version: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum RequestBody {
    Raw(String),
    Json(serde_json::Value),
    File(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthInfo {
    username: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResponseInfo {
    request_id: String,
    status: Option<u16>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    error: Option<String>,
    completed: bool,
}

// Lua module functions
#[mlua::lua_module]
fn avante_curl(lua: &Lua) -> LuaResult<LuaTable> {
    // Create module table
    let exports = lua.create_table()?;

    // Register functions
    exports.set("create_session", lua.create_function(create_session)?)?;
    exports.set("destroy_session", lua.create_function(destroy_session)?)?;
    exports.set("request", lua.create_function(request)?)?;
    exports.set("get", lua.create_function(get)?)?;
    exports.set("post", lua.create_function(post)?)?;
    exports.set("put", lua.create_function(put)?)?;
    exports.set("delete", lua.create_function(delete)?)?;
    exports.set("head", lua.create_function(head)?)?;
    exports.set("patch", lua.create_function(patch)?)?;
    exports.set("get_status", lua.create_function(get_status)?)?;
    exports.set("cancel_request", lua.create_function(cancel_request)?)?;

    Ok(exports)
}

// Create a new session
fn create_session(_: &Lua, _: ()) -> LuaResult<String> {
    let session_id = Uuid::new_v4().to_string();
    let session = Arc::new(Session::new());
    SESSIONS.insert(session_id.clone(), session);
    Ok(session_id)
}

// Destroy an existing session
fn destroy_session(_: &Lua, session_id: String) -> LuaResult<bool> {
    Ok(SESSIONS.remove(&session_id).is_some())
}

// Make a request with given options
fn request(_: &Lua, (session_id, request_id, options): (String, String, LuaTable)) -> LuaResult<String> {
    let req_options: RequestOptions = options
        .get("_options")
        .map_err(|_| LuaError::RuntimeError("Invalid options".to_string()))?;

    // Get the session
    let session = SESSIONS
        .get(&session_id)
        .ok_or_else(|| LuaError::RuntimeError(format!("Session not found: {}", session_id)))?
        .clone();

    let cloned_id = request_id.clone();

    RUNTIME.spawn(async move {
        if let Err(e) = execute_request(&session, &cloned_id, req_options).await {
            session.set_error(&cloned_id, &e.to_string());
        }
        session.set_completed(&cloned_id);
    });

    Ok(request_id.clone())
}

// Convenience function for GET requests
fn get(lua: &Lua, (session_id, url, opts): (String, String, Option<LuaTable>)) -> LuaResult<String> {
    let opts_table = match opts {
        Some(t) => t,
        None => lua.create_table()?,
    };

    opts_table.set("method", "GET")?;
    opts_table.set("url", url)?;

    // Generate a unique request ID
    let request_id = format!("{}", Uuid::new_v4());

    request(lua, (session_id, request_id, opts_table))?
    Ok(request_id)
}

// Convenience function for POST requests
fn post(lua: &Lua, (session_id, url, opts): (String, String, Option<LuaTable>)) -> LuaResult<String> {
    let opts_table = match opts {
        Some(t) => t,
        None => lua.create_table()?,
    };

    opts_table.set("method", "POST")?;
    opts_table.set("url", url)?;

    // Generate a unique request ID
    let request_id = format!("{}", Uuid::new_v4());

    request(lua, (session_id, request_id, opts_table))?
}

// Convenience function for PUT requests
fn put(lua: &Lua, (session_id, url, opts): (String, String, Option<LuaTable>)) -> LuaResult<String> {
    let opts_table = match opts {
        Some(t) => t,
        None => lua.create_table()?,
    };

    opts_table.set("method", "PUT")?;
    opts_table.set("url", url)?;

    // Generate a unique request ID
    let request_id = format!("{}", Uuid::new_v4());

    request(lua, (session_id, request_id, opts_table))?
}

// Convenience function for DELETE requests
fn delete(lua: &Lua, (session_id, url, opts): (String, String, Option<LuaTable>)) -> LuaResult<String> {
    let opts_table = match opts {
        Some(t) => t,
        None => lua.create_table()?,
    };

    opts_table.set("method", "DELETE")?;
    opts_table.set("url", url)?;

    // Generate a unique request ID
    let request_id = format!("{}", Uuid::new_v4());

    request(lua, (session_id, request_id, opts_table))?
}

// Convenience function for HEAD requests
fn head(lua: &Lua, (session_id, url, opts): (String, String, Option<LuaTable>)) -> LuaResult<String> {
    let opts_table = match opts {
        Some(t) => t,
        None => lua.create_table()?,
    };

    opts_table.set("method", "HEAD")?;
    opts_table.set("url", url)?;

    // Generate a unique request ID
    let request_id = format!("{}", Uuid::new_v4());

    request(lua, (session_id, request_id, opts_table))?
}

// Convenience function for PATCH requests
fn patch(lua: &Lua, (session_id, url, opts): (String, String, Option<LuaTable>)) -> LuaResult<String> {
    let opts_table = match opts {
        Some(t) => t,
        None => lua.create_table()?,
    };

    opts_table.set("method", "PATCH")?;
    opts_table.set("url", url)?;

    // Generate a unique request ID
    let request_id = format!("{}", Uuid::new_v4());

    request(lua, (session_id, request_id, opts_table))?
}

// Get status of a request
fn get_status(lua: &Lua, (session_id, request_id): (String, String)) -> LuaResult<LuaTable> {
    // print all the sessions
    SESSIONS.iter().for_each(|session| {
        println!("Session ID: {} - {:?}", session.key(), session.value());
    });

    // Check if the session exists
    if !SESSIONS.contains_key(&session_id) {
        return Err(LuaError::RuntimeError(format!(
            "Session not found: {}",
            session_id
        )));
    }

    // Get the session and request info
    let session = SESSIONS
        .get(&session_id)
        .ok_or_else(|| LuaError::RuntimeError(format!("Session not found: {}", session_id)))?;

    let response_info = session.get_response(&request_id);

    let response = match serde_json::to_string(&response_info) {
        Ok(json) => json,
        Err(e) => {
            return Err(LuaError::RuntimeError(format!(
                "Failed to serialize response info: {}",
                e
            )))
        }
    };

    // Convert the ResponseInfo to a Lua table directly
    let table = lua.create_table()?;

    // Add fields from ResponseInfo to the table
    if let Some(status) = response_info.status {
        table.set("status", status)?;
    }

    if let Some(headers) = &response_info.headers {
        let headers_table = lua.create_table()?;
        for (k, v) in headers {
            headers_table.set(k.clone(), v.clone())?;
        }
        table.set("headers", headers_table)?;
    }

    if let Some(body) = &response_info.body {
        table.set("body", body.clone())?;
    }

    if let Some(error) = &response_info.error {
        table.set("error", error.clone())?;
    }

    table.set("completed", response_info.completed)?;

    Ok(table)
}

// Cancel an in-progress request
fn cancel_request(_: &Lua, (session_id, request_id): (String, String)) -> LuaResult<bool> {
    let session = match SESSIONS.get(&session_id) {
        Some(s) => s,
        None => return Ok(false),
    };

    session.cancel_request(&request_id);
    Ok(true)
}

// Execute the request asynchronously
async fn execute_request(
    session: &Session,
    request_id: &str,
    options: RequestOptions,
) -> Result<(), anyhow::Error> {
    let client = HttpClient::new_from_options(&options)?;
    let response = client.send_request(options).await?;

    // Process response headers
    let mut headers_map = HashMap::new();
    // let var_name =  response.headers();
    // if var_name.is_some() {
    for (key, value) in response.headers().iter() {
        if let Ok(val_str) = value.to_str() {
            headers_map.insert(key.as_str().to_string(), val_str.to_string());
        }
    }
    // }

    // Process response body
    let status = response.status().as_u16();
    let body = response.text().await?;

  println!("request_id: {} status: {} body: {}", request_id, status, body);

    session.set_response(request_id, status, headers_map, &body);

    Ok(())
}















