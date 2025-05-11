-- Unit tests for curl_client using plenary.async to match integration_spec style
require("plenary.async").tests.add_to_env()
local curl_client = require("avante.curl_client")
local async = require("plenary.async")
local assert = require("luassert")
local uv = vim.uv or vim.loop

a.describe("AvanteCurlClient", function()
  local client
  local mock_curl

  a.before_each(function()
    -- Create a client instance for testing
    client = curl_client.create()

    -- Store reference to request_map for assertions
    mock_curl = package.loaded["avante_curl"]
  end)

  a.after_each(function()
    -- Clean up after each test
    client:destroy()
  end)

  a.it("initializes a request with the correct state", function()
    -- Create a request
    local request_id = client:request({
      url = "http://example.com",
      method = "GET",
    })

    -- Assert the request was created with the correct state
    assert.is_not_nil(client.request_map[request_id])
    assert.equals("Init", client.request_map[request_id].state)
  end)

  a.it("handles state transitions", function()
    -- Create a request
    local request_id = client:request({
      url = "http://example.com",
      method = "GET",
    })

    -- Simulate state transitions by updating the backend response
    -- Setup mock responses for different states
    local states = {"Sending", "Receiving", "Complete"}

    for _, state in ipairs(states) do
      -- Modify the request state
      client.request_map[request_id].state = state

      -- Process the request
      client:poll_requests()

      -- Verify the state was updated
      assert.equals(state, client.request_map[request_id].state)

      -- If we're in a terminal state, it would be removed from request_map
      if state == "Complete" or state == "Error" or state == "Cancelled" then
        async.util.sleep(100) -- Allow time for processing
      end
    end
  end)

  a.it("invokes callbacks correctly", function()
    local complete_called = false
    local error_called = false
    local chunk_called = false

    -- Define callback functions
    local on_complete = function()
      complete_called = true
    end

    local on_error = function()
      error_called = true
    end

    local on_chunk = function()
      chunk_called = true
    end

    -- Test complete callback
    local complete_id = client:request({
      url = "http://example.com/complete",
      method = "GET",
      on_complete = on_complete,
    })

    -- Simulate completion
    client.request_map[complete_id].state = "Complete"
    client:poll_requests()
    async.util.sleep(100) -- Give time for callback execution
    assert.is_true(complete_called)

    -- Test error callback
    local error_id = client:request({
      url = "http://example.com/error",
      method = "GET",
      on_error = on_error,
    })

    -- Simulate error
    client.request_map[error_id].state = "Error"
    client.request_map[error_id].error = "Test error"
    client:poll_requests()
    async.util.sleep(100) -- Give time for callback execution
    assert.is_true(error_called)

    -- Test chunk callback with streaming
    local chunk_id = client:request({
      url = "http://example.com/stream",
      method = "GET",
      on_chunk = on_chunk,
    })

    -- Simulate receiving chunks
    client.request_map[chunk_id].last_body = "initial data"
    client.request_map[chunk_id].last_body_length = 12

    -- Update with new data to trigger chunk callback
    mock_curl.get_status = function()
      return {
        body = "initial data new chunk",
        state = "Receiving",
        completed = false
      }
    end

    client:poll_requests()
    async.util.sleep(100) -- Give time for callback

    -- Reset mock
    mock_curl.get_status = function() return { completed = false } end

    assert.is_true(chunk_called)
  end)

  a.it("cancels requests", function()
    -- Create a request
    local request_id = client:request({
      url = "http://example.com/cancel",
      method = "GET",
    })

    -- Cancel the request
    client:cancel(request_id)

    -- Wait for cancellation to process
    async.util.sleep(100)

    -- The request should be marked as cancelled
    assert.equals("Cancelled", client.request_map[request_id].state)
  end)

  a.it("handles timeouts", function()
    -- Create a request
    local request_id = client:request({
      url = "http://example.com/timeout",
      method = "GET",
      timeout = 1, -- Short timeout for testing
    })

    -- Initialize the request
    client.request_map[request_id].state = "Sending"

    -- Simulate timeout by not updating state for a while
    async.util.sleep(1200) -- Wait longer than the timeout

    -- Process the timeout
    client:poll_requests()

    -- Request should be moved to timeout state
    assert.equals("Timeout", client.request_map[request_id].state)
  end)

  a.it("clears completed requests from request_map", function()
    -- Create a request
    local request_id = client:request({
      url = "http://example.com/complete",
      method = "GET",
    })

    -- Verify request exists in map
    assert.is_not_nil(client.request_map[request_id])

    -- Mark as complete
    mock_curl.get_status = function()
      return {
        completed = true,
        state = "Complete",
        status = 200
      }
    end

    -- Poll to process completion
    client:poll_requests()
    async.util.sleep(100) -- Allow time for cleanup

    -- Request should be removed from map
    assert.is_nil(client.request_map[request_id])

    -- Reset mock
    mock_curl.get_status = function() return { completed = false } end
  end)

  a.it("handles streams correctly", function()
    local chunks_received = {}

    -- Create a streaming request
    local request_id = client:request({
      url = "http://example.com/stream",
      method = "GET",
      stream = true,
      on_chunk = function(chunk)
        table.insert(chunks_received, chunk)
      end,
    })

    -- Simulate receiving streaming data in chunks
    local total_chunks = 3
    for i = 1, total_chunks do
      -- Update the mock to return incremental chunks
      local body = string.rep("data", i)
      mock_curl.get_status = function()
        return {
          body = body,
          state = "Receiving",
          completed = (i == total_chunks)
        }
      end

      client.request_map[request_id].last_body = (i > 1) and string.rep("data", i-1) or ""
      client.request_map[request_id].last_body_length = (i > 1) and #(string.rep("data", i-1)) or 0

      -- Process the chunk
      client:poll_requests()
      async.util.sleep(50)
    end

    -- Reset mock
    mock_curl.get_status = function() return { completed = false } end

    -- Verify chunks were processed
    assert.equals(total_chunks, #chunks_received)
    assert.equals("data", chunks_received[1])
    assert.equals("data", chunks_received[2])
    assert.equals("data", chunks_received[3])
  end)
end)

