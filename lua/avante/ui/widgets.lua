
local Highlights = require("avante.highlights")

local M = {}

-- Function to create a button
---@param text string
---@param focus boolean
---@return string
function M.button(text, focus)
  local highlight = focus and Highlights.BUTTON_DEFAULT_HOVER or Highlights.BUTTON_DEFAULT
  return " [" .. text .. "] "
end

-- Function to create a label
---@param text string
---@return string
function M.label(text)
  return text
end

-- Function to create an input field
---@param text string
---@return string
function M.input(text)
  return text
end

return M

