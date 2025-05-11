local uv = vim.uv or vim.loop
local Config = require("avante.config")

-- Define request states to match the Rust backend
local RequestState = {
  Init = "Init",               -- Request is initialized but not started
  Sending = "Sending",         -- Request is being sent
  Receiving = "Receiving",     -- Request is receiving data
  Complete = "Complete",       -- Request completed successfully
  Error = "Error",             -- Request encountered an error
  Timeout = "Timeout",         -- Request timed out
  Cancelled = "Cancelled",     -- Request was cancelled
  Idle = "Idle",               -- Request is idle (no recent activity)
  Acknowledged = "Acknowledged" -- Request completion was acknowledged by client
}

-- Lazy load the avante-curl module
local avante_curl = nil
local function load_avante_curl()
  if avante_curl then return avante_curl end

  local ok, curl = pcall(require, "avante_curl")
  if not ok then error("Failed to load avante_curl module. Make sure the Rust crate is built: " .. tostring(curl)) end

  avante_curl = curl
  return curl
end

-- Helper function to handle callbacks in a safe way
local function safe_callback(callback, ...)
  if not callback then return end

  local status, err = pcall(callback, ...)
  if not status then
    vim.schedule(function() vim.notify("Error in callback: " .. tostring(err), vim.log.levels.ERROR) end)
  end
end

-- Helper function to resume a coroutine safely
local function safe_resume(thread, ...)
  if not thread then return end

  if coroutine.status(thread) == "dead" then
    vim.schedule(function() vim.notify("Attempting to resume dead coroutine", vim.log.levels.WARN) end)
    return
  end

  local status, err = coroutine.resume(thread, ...)
  if not status then
    vim.schedule(function() vim.notify("Error in coroutine: " .. tostring(err), vim.log.levels.ERROR) end)
  end
end

---@class AvanteCurlClient
---@field session_id string
---@field request_map table<string, table>
---@field polling_interval number
---@field polling_timer table
local AvanteCurlClient = {}

function AvanteCurlClient.new()
  local curl = load_avante_curl()
  local session_id = curl.create_session()

  local self = setmetatable({
    session_id = session_id,
    request_map = {},
    polling_interval = 100, -- milliseconds
    polling_timer = nil,
  }, { __index = AvanteCurlClient })

  -- Start the polling timer
  self:start_polling()

  return self
end

function AvanteCurlClient:destroy()
  if self.polling_timer then
    self.polling_timer:stop()
    self.polling_timer:close()
    self.polling_timer = nil
  end

  local curl = load_avante_curl()
  curl.destroy_session(self.session_id)
  self.session_id = nil
  self.request_map = {}
end

function AvanteCurlClient:start_polling()
  if self.polling_timer then
    self.polling_timer:stop()
    self.polling_timer:close()
  end

  self.polling_timer = uv.new_timer()
  self.polling_timer:start(0, self.polling_interval, vim.schedule_wrap(function() self:poll_requests() end))
end

function AvanteCurlClient:poll_requests()
  local curl = load_avante_curl()

  -- Only poll for status updates, not for callback handling
  for request_id, request_info in pairs(self.request_map) do
    local status = curl.get_status(self.session_id, request_id)

    -- Update request state based on backend response
    if status.state then
      request_info.state = status.state
    end

    -- Check if the request is in a terminal state
    local is_terminal_state = status.state == RequestState.Complete
      or status.state == RequestState.Error
      or status.state == RequestState.Timeout
      or status.state == RequestState.Cancelled
      or status.state == RequestState.Idle

    if is_terminal_state then
      -- Remove from the request map only if not using callbacks
      -- Callbacks will be handled directly by the Rust backend now
      if not (request_info.on_complete or request_info.on_error or request_info.on_chunk) then
        self.request_map[request_id] = nil
      end
    end
  end
end

-- Generic request method
function AvanteCurlClient:request(options)
  local curl = load_avante_curl()

  local opts = vim.tbl_deep_extend("force", {
    url = "",
    method = "GET",
    headers = {},
    body = nil,
    query = nil,
    form = nil,
    auth = nil,
    timeout = 60,
    insecure = false,
    proxy = nil,
    stream = nil,
    on_complete = nil,
    on_error = nil,
    on_chunk = nil,
  }, options or {})

  -- Generate a unique request ID
  local request_id = vim.fn.sha256(opts.url .. tostring(vim.fn.localtime()) .. vim.fn.rand())

  local lua_opts = {
    _options = {
      url = opts.url,
      method = opts.method,
      headers = opts.headers,
      timeout = opts.timeout,
      insecure = opts.insecure,
      proxy = opts.proxy,
    },
    _callbacks = {
      -- Pass callback functions directly to Rust
      on_complete = opts.on_complete,
      on_error = opts.on_error,
      on_chunk = opts.on_chunk,  -- Can be a coroutine/thread
    },
  }

  -- Add body if present
  if opts.body then
    if type(opts.body) == "table" then
      lua_opts._options.body = { Json = vim.json.encode(opts.body) }
    elseif type(opts.body) == "string" and vim.fn.filereadable(opts.body) == 1 then
      lua_opts._options.body = { File = opts.body }
    else
      lua_opts._options.body = { Raw = tostring(opts.body) }
    end
  end

  -- Add query params if present
  if opts.query then lua_opts._options.query = opts.query end

  -- Add form data if present
  if opts.form then lua_opts._options.form = opts.form end

  -- Add auth info if present
  if opts.auth then lua_opts._options.auth = opts.auth end

  -- Store request info for status tracking
  self.request_map[request_id] = {
    id = request_id,
    on_complete = opts.on_complete,
    on_error = opts.on_error,
    on_chunk = opts.on_chunk,
    state = RequestState.Init, -- Initialize with Init state
  }

  -- Make the request and pass the request_id to the backend
  curl.request(self.session_id, request_id, lua_opts)

  return request_id
end

-- Helper methods for common HTTP methods
function AvanteCurlClient:get(url, opts)
  local options = vim.tbl_extend("force", opts or {}, {
    url = url,
    method = "GET",
  })
  return self:request(options)
end

function AvanteCurlClient:post(url, opts)
  local options = vim.tbl_extend("force", opts or {}, {
    url = url,
    method = "POST",
  })
  return self:request(options)
end

function AvanteCurlClient:put(url, opts)
  local options = vim.tbl_extend("force", opts or {}, {
    url = url,
    method = "PUT",
  })
  return self:request(options)
end

function AvanteCurlClient:delete(url, opts)
  local options = vim.tbl_extend("force", opts or {}, {
    url = url,
    method = "DELETE",
  })
  return self:request(options)
end

function AvanteCurlClient:head(url, opts)
  local options = vim.tbl_extend("force", opts or {}, {
    url = url,
    method = "HEAD",
  })
  return self:request(options)
end

function AvanteCurlClient:patch(url, opts)
  local options = vim.tbl_extend("force", opts or {}, {
    url = url,
    method = "PATCH",
  })
  return self:request(options)
end

function AvanteCurlClient:cancel(request_id)
  local curl = load_avante_curl()

  -- Update the local state to reflect cancellation
  if self.request_map[request_id] then
    self.request_map[request_id].state = RequestState.Cancelled
  end

  -- Call the backend to cancel the request
  return curl.cancel_request(self.session_id, request_id)
end

-- Create a singleton client instance for global use
local singleton_client = nil

local M = {
  create = function() return AvanteCurlClient.new() end,

  -- Get the singleton client
  get_client = function()
    if not singleton_client then singleton_client = AvanteCurlClient.new() end
    return singleton_client
  end,

  -- Request methods that use the singleton client
  request = function(opts) return M.get_client():request(opts) end,

  get = function(url, opts) return M.get_client():get(url, opts) end,

  post = function(url, opts) return M.get_client():post(url, opts) end,

  put = function(url, opts) return M.get_client():put(url, opts) end,

  delete = function(url, opts) return M.get_client():delete(url, opts) end,

  head = function(url, opts) return M.get_client():head(url, opts) end,

  patch = function(url, opts) return M.get_client():patch(url, opts) end,

  cancel = function(request_id) return M.get_client():cancel(request_id) end,
}

return M


