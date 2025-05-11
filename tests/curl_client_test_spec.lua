local busted = require('plenary.busted')
local assert = require('luassert')
local mock = require('luassert.mock')
local stub = require('luassert.stub')
local match = require('luassert.match')
require('plenary.async').tests.add_to_env()
a.describe('curl_client', function()
  -- Setup test environment
  local avante_curl_mock
  local uv_mock
  local curl_client

  -- Reset mocks before each test
  a.before_each(function()
    -- Mock dependencies
    avante_curl_mock = mock({
      create_session = function() return "mock_session_id" end,
      destroy_session = function() return true end,
      request = function() return "mock_request_id" end,
      get_status = function() return { completed = false } end,
      cancel_request = function() return true end,
    }, true)

    -- Mock timer
    uv_mock = {
      new_timer = function()
        return {
          start = function() end,
          stop = function() end,
          close = function() end,
        }
      end
    }

    -- Set up testing environment
    _G.vim = {
      schedule_wrap = function(fn) return fn end,
      tbl_deep_extend = function(_, _, a, b) return vim.tbl_extend('force', a, b) end,
      tbl_extend = function(_, a, b)
        local result = {}
        for k, v in pairs(a) do result[k] = v end
        for k, v in pairs(b) do result[k] = v end
        return result
      end,
      notify = function() end,
      log = { levels = { ERROR = 1 } }
    }

    -- Override the curl module loader to return our mock
    package.loaded["avante_curl"] = avante_curl_mock
    package.loaded["vim.uv"] = uv_mock

    -- Load the module under test
    package.loaded["avante.curl_client"] = nil
    curl_client = require('avante.curl_client')
  end)

  a.after_each(function()
    package.loaded["avante_curl"] = nil
    package.loaded["vim.uv"] = nil
  end)

  a.describe('client creation', function()
    it('should create a new client instance', function()
      local client = curl_client.create()
      assert.is_not_nil(client)
      assert.equals("mock_session_id", client.session_id)
    end)

    it('should return a singleton client instance', function()
      local client1 = curl_client.get_client()
      local client2 = curl_client.get_client()
      assert.equals(client1, client2)
    end)
  end)

  a.describe('request methods', function()
    a.it('should make GET requests', function()
      local client = curl_client.create()
      stub(avante_curl_mock, 'request')

      client:get("https://example.com", {
        headers = { ["User-Agent"] = "Avante-Test" }
      })

      assert.stub(avante_curl_mock.request).was.called_with(
        "mock_session_id",
        match.has_field("_options", match.has_fields({
          url = "https://example.com",
          method = "GET"
        }))
      )

      avante_curl_mock.request:revert()
    end)

    a.it('should make POST requests', function()
      local client = curl_client.create()
      stub(avante_curl_mock, 'request')

      client:post("https://example.com", {
        body = { test = "data" }
      })

      assert.stub(avante_curl_mock.request).was.called_with(
        "mock_session_id",
        match.has_field("_options", match.has_fields({
          url = "https://example.com",
          method = "POST"
        }))
      )

      avante_curl_mock.request:revert()
    end)
  end)

  a.describe('polling mechanism', function()
    a.it('should handle completed requests', function()
      local client = curl_client.create()

      -- Setup request info
      client.request_map["test_request"] = {
        on_complete = spy.new(function() end),
        on_error = spy.new(function() end),
        last_body = "",
        last_body_length = 0
      }

      -- Mock a completed request
      avante_curl_mock.get_status = function()
        return {
          completed = true,
          status = 200,
          body = "success",
          headers = { ["Content-Type"] = "text/plain" }
        }
      end

      -- Trigger polling
      client:poll_requests()

      -- Verify callbacks
      assert.spy(client.request_map["test_request"].on_complete).was.called(1)
      assert.spy(client.request_map["test_request"].on_error).was_not_called()
      assert.is_nil(client.request_map["test_request"]) -- should be removed
    end)

    a.it('should handle error responses', function()
      local client = curl_client.create()

      -- Setup request info
      client.request_map["test_request"] = {
        on_complete = spy.new(function() end),
        on_error = spy.new(function() end),
        last_body = "",
        last_body_length = 0
      }

      -- Mock an error response
      avante_curl_mock.get_status = function()
        return {
          completed = true,
          error = "Connection refused"
        }
      end

      -- Trigger polling
      client:poll_requests()

      -- Verify callbacks
      assert.spy(client.request_map["test_request"].on_complete).was_not_called()
      assert.spy(client.request_map["test_request"].on_error).was.called(1)
      assert.is_nil(client.request_map["test_request"]) -- should be removed
    end)

    a.it('should handle streaming data', function()
      local client = curl_client.create()

      -- Setup request info with streaming handler
      client.request_map["test_request"] = {
        on_chunk = spy.new(function() end),
        last_body = "initial",
        last_body_length = 7
      }

      -- Mock a streaming response
      avante_curl_mock.get_status = function()
        return {
          completed = false,
          body = "initial data chunk"
        }
      end

      -- Trigger polling
      client:poll_requests()

      -- Verify streaming callback with just the new part
      assert.spy(client.request_map["test_request"].on_chunk).was.called_with(" data chunk")
    end)
  end)

  a.describe('cleanup', function()
    it('should properly destroy the client', function()
      local client = curl_client.create()
      stub(avante_curl_mock, 'destroy_session')

      client:destroy()

      assert.stub(avante_curl_mock.destroy_session).was.called_with("mock_session_id")
      assert.is_nil(client.session_id)
      assert.same({}, client.request_map)
    end)
  end)

  a.describe('global API', function()
    a.it('should provide global request methods', function()
      stub(avante_curl_mock, 'request')

      curl_client.get("https://example.com")

      assert.stub(avante_curl_mock.request).was.called()

      avante_curl_mock.request:revert()
    end)
  end)
end)
