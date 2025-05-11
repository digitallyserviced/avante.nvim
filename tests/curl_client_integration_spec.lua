-- This is an integration test that requires the avante_curl module to be built
-- Run this test only after building the Rust crate
require("plenary.async").tests.add_to_env()
local assert = require("luassert")

local async = require("plenary.async")

a.describe("curl_client integration", function()
  local curl_client

  a.before_each(function()
    -- Load the actual module (not mocked)
    curl_client = require("avante.curl_client")
  end)

  a.after_each(function()
    -- Clean up the singleton instance if it exists
    if curl_client._singleton_client then
      curl_client._singleton_client:destroy()
      curl_client._singleton_client = nil
    end
  end)

  a.it("should create a client and make an HTTP request", function()
    async.run(function()
      local client = curl_client.create()
      assert.is_not_nil(client)

      local response_received = false
      local response_data = nil
      local error_data = nil

      -- Make an actual HTTP request to a test endpoint
      local request_id = client:get("https://httpbin.org/get", {
        timeout = 10, -- Increased timeout
        headers = {
          ["User-Agent"] = "Avante-Test-Agent",
        },
        on_complete = function(response)
          print("Response received: " .. response.body)
          response_received = true
          response_data = response
        end,
        on_error = function(error)
          print("Request failed: " .. (error.message or "unknown error"))
          error_data = error
          response_received = true
        end,
      })

      -- Wait for the response with a timeout
      local timeout = os.time() + 15 -- Increased to 15 seconds timeout
      while not response_received and os.time() < timeout do
        -- Poll manually to simulate event loop
        client:poll_requests()
        async.util.sleep(100)
      end

      -- Verify we got a response
      assert.is_nil(error_data)
      assert.is_true(response_received)
      assert.is_not_nil(response_data)

      -- Verify response
      assert.equals(200, response_data.status)
      assert.is_not_nil(response_data.body)

      -- Parse the JSON response body
      local ok, body = pcall(vim.fn.json_decode, response_data.body)
      assert.is_true(ok)

      -- Verify the User-Agent was correctly sent
      assert.equals("Avante-Test-Agent", body.headers["User-Agent"])

      client:destroy()
    end)
  end)

  a.it("should handle streaming responses", function()
    -- async.run(function()
    local client = curl_client.create()

    local chunks = {}
    local done = false
    local error_data = nil

    -- Make a request to an endpoint that streams data
    client:get("https://httpbin.org/stream/5", {
      timeout = 10,
      stream = true,
      on_chunk = function(chunk)
        print("Received chunk: " .. chunk)
        table.insert(chunks, chunk)
      end,
      on_complete = function() done = true end,
      on_error = function(error)
        error_data = error
        done = true
        -- print("Request failed: " .. (error.message or "unknown error"))
      end,
    })

    -- Wait for completion
    local timeout = os.time() + 15 -- 15 seconds timeout
    while not done and os.time() < timeout do
      client:poll_requests()
      async.util.sleep(100)
    end

    -- Verify we received multiple chunks
    assert.is_true(#chunks > 0)
    assert.is_true(done)

    client:destroy()
    -- end)
  end)

  a.it("should handle errors correctly", function()
    -- async.run(function()
    local client = curl_client.create()

    local error_received = false
    local error_data = nil

    -- Make a request to a non-existent domain
    client:get("https://this-domain-definitely-does-not-exist-12345.com", {
      timeout = 5,
      on_complete = function() error("Expected error, got success") end,
      on_error = function(error)
        error_received = true
        error_data = error
      end,
    })

    -- Wait for the error
    local timeout = os.time() + 10
    while not error_received and os.time() < timeout do
      client:poll_requests()
      async.util.sleep(100)
    end

    -- Verify we got an error
    assert.is_true(error_received)
    assert.is_not_nil(error_data)
    assert.is_not_nil(error_data.message)

    client:destroy()
    -- end)
  end)

  a.it("should cancel requests", function()
    -- async.run(function()
    local client = curl_client.create()

    local request_cancelled = false

    -- Start a request
    local request_id = client:get("https://httpbin.org/delay/5", {
      timeout = 10,
      on_error = function(error) request_cancelled = error.message:find("cancelled") ~= nil end,
    })

    -- Wait briefly then cancel it
    async.util.sleep(100)
    client:cancel(request_id)

    -- Wait a bit to ensure the cancellation is processed
    local timeout = os.time() + 5
    while not request_cancelled and os.time() < timeout do
      client:poll_requests()
      async.util.sleep(100)
    end

    -- Verify the request was cancelled
    assert.is_true(request_cancelled)

    client:destroy()
    -- end)
  end)
end)

