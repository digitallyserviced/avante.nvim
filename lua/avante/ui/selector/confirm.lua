local api = vim.api
local Line = require("avante.ui.line")
local Utils = require("avante.utils")
local Highlights = require("avante.highlights")
local Selector = require("avante.ui.selector")

---@class avante.ui.SelectorConfirm
---@field id string Unique ID for this confirmation
---@field message string Message to display
---@field callback function Callback function for button actions
local Confirm = {}
Confirm.__index = Confirm

---@param opts { message: string, callback: fun(type: "yes"|"all"|"no", reason?: string), bufnr: number, line_nr: number }
---@return avante.ui.SelectorConfirm
function Confirm:new(opts)
    local this = setmetatable({}, self)
    this.id = "confirm_" .. tostring(opts.bufnr) .. "_" .. tostring(opts.line_nr) .. "_" .. os.time()
    this.message = opts.message
    this.callback = opts.callback
    this.bufnr = opts.bufnr
    this.line_nr = opts.line_nr
    this.focus_index = 3  -- Default to "No" (1=Yes, 2=All, 3=No)
    this.extmark_ids = {}
    
    return this
end

--- Open the confirmation dialog
function Confirm:open()
    -- Clear any existing widget with the same ID
    Selector.clear_widget(self.id)
    
    -- Create lines from message
    local content_lines = {}
    
    -- Add empty line for spacing
    table.insert(content_lines, "")
    
    -- Add message lines
    for _, line in ipairs(vim.split(self.message, "\n")) do
        table.insert(content_lines, line)
    end
    
    -- Create buttons line
    local BUTTON_NORMAL = Highlights.BUTTON_DEFAULT
    local BUTTON_FOCUS = Highlights.BUTTON_DEFAULT_HOVER
    
    local buttons_line = Line:new({
        { " [Y]es ", function() return self.focus_index == 1 and BUTTON_FOCUS or BUTTON_NORMAL end },
        { "   " },
        { " [A]ll yes ", function() return self.focus_index == 2 and BUTTON_FOCUS or BUTTON_NORMAL end },
        { "    " },
        { " [N]o ", function() return self.focus_index == 3 and BUTTON_FOCUS or BUTTON_NORMAL end },
    })
    
    -- Add buttons
    table.insert(content_lines, "")
    table.insert(content_lines, buttons_line)
    table.insert(content_lines, "")
    
    -- Create the widget
    local extmark_id = Selector.create_virt_lines(self.bufnr, self.line_nr, content_lines, {
        above = false,
        priority = 200
    })
    
    -- Register this widget 
    table.insert(self.extmark_ids, extmark_id)
    
    -- Add keymaps
    self:_setup_keymaps()
    
    -- Register widget for cleanup
    Selector.register_widget(self.id, {
        bufnr = self.bufnr,
        extmark_ids = self.extmark_ids,
        cleanup = function() self:_cleanup_keymaps() end
    })
    
    return self.id
end

--- Update the button highlights based on current focus
function Confirm:render()
    if #self.extmark_ids == 0 then return end
    
    -- Get the current position data
    local pos = Selector.get_extmark_position(self.bufnr, self.extmark_ids[1])
    if not pos then return end
    
    -- Delete the existing extmark
    pcall(api.nvim_buf_del_extmark, self.bufnr, Selector.ns_id, self.extmark_ids[1])
    
    -- Create new content with updated focus
    local content_lines = {}
    
    -- Add empty line for spacing
    table.insert(content_lines, "")
    
    -- Add message lines
    for _, line in ipairs(vim.split(self.message, "\n")) do
        table.insert(content_lines, line)
    end
    
    -- Create buttons line
    local BUTTON_NORMAL = Highlights.BUTTON_DEFAULT
    local BUTTON_FOCUS = Highlights.BUTTON_DEFAULT_HOVER
    
    local buttons_line = Line:new({
        { " [Y]es ", function() return self.focus_index == 1 and BUTTON_FOCUS or BUTTON_NORMAL end },
        { "   " },
        { " [A]ll yes ", function() return self.focus_index == 2 and BUTTON_FOCUS or BUTTON_NORMAL end },
        { "    " },
        { " [N]o ", function() return self.focus_index == 3 and BUTTON_FOCUS or BUTTON_NORMAL end },
    })
    
    -- Add buttons
    table.insert(content_lines, "")
    table.insert(content_lines, buttons_line)
    table.insert(content_lines, "")
    
    -- Create new extmark
    local new_extmark_id = Selector.create_virt_lines(self.bufnr, self.line_nr, content_lines, {
        above = false,
        priority = 200
    })
    
    -- Update extmark ID in our state
    self.extmark_ids = { new_extmark_id }
    
    -- Update the widget registration
    Selector.active_widgets[self.id].extmark_ids = self.extmark_ids
end

--- Handle button click
function Confirm:click_button()
    if self.focus_index == 1 then
        -- Yes button
        self.callback("yes")
        Selector.clear_widget(self.id)
        return
    end
    
    if self.focus_index == 2 then
        -- All Yes button
        Utils.notify("Accept all")
        self.callback("all")
        Selector.clear_widget(self.id)
        return
    end
    
    -- No button - with reason input
    self:_prompt_no_reason()
end

--- Prompt for "No" reason
function Confirm:_prompt_no_reason()
    -- Clean up confirm widget
    Selector.clear_widget(self.id)
    
    -- Create input prompt
    local Input = require("avante.ui.selector.input")
    local input = Input:new({
        bufnr = self.bufnr,
        line_nr = self.line_nr,
        prompt = "Rejection reason: ",
        placeholder = "Optional reason for rejecting",
        callback = function(input_text)
            self.callback("no", input_text ~= "" and input_text or nil)
        end
    })
    
    input:open()
end

--- Setup keymaps for the confirm dialog
function Confirm:_setup_keymaps()
    -- Create unique keymaps for this widget
    local keymap_opts = { buffer = self.bufnr, nowait = true }
    
    -- Yes keymaps
    vim.keymap.set("n", "y", function() 
        self.focus_index = 1
        self:render()
        self:click_button()
    end, keymap_opts)
    vim.keymap.set("n", "Y", function() 
        self.focus_index = 1
        self:render()
        self:click_button()
    end, keymap_opts)
    
    -- All keymaps
    vim.keymap.set("n", "a", function() 
        self.focus_index = 2
        self:render()
        self:click_button()
    end, keymap_opts)
    vim.keymap.set("n", "A", function() 
        self.focus_index = 2
        self:render()
        self:click_button()
    end, keymap_opts)
    
    -- No keymaps
    vim.keymap.set("n", "n", function() 
        self.focus_index = 3
        self:render()
        self:click_button()
    end, keymap_opts)
    vim.keymap.set("n", "N", function() 
        self.focus_index = 3
        self:render()
        self:click_button()
    end, keymap_opts)
    
    -- Navigation keymaps
    vim.keymap.set("n", "<Left>", function() 
        self.focus_index = self.focus_index - 1
        if self.focus_index < 1 then self.focus_index = 3 end
        self:render()
    end, keymap_opts)
    
    vim.keymap.set("n", "<Right>", function() 
        self.focus_index = self.focus_index + 1
        if self.focus_index > 3 then self.focus_index = 1 end
        self:render()
    end, keymap_opts)
    
    vim.keymap.set("n", "<Tab>", function() 
        self.focus_index = self.focus_index + 1
        if self.focus_index > 3 then self.focus_index = 1 end
        self:render()
    end, keymap_opts)
    
    vim.keymap.set("n", "<S-Tab>", function() 
        self.focus_index = self.focus_index - 1
        if self.focus_index < 1 then self.focus_index = 3 end
        self:render()
    end, keymap_opts)
    
    -- Enter to select current option
    vim.keymap.set("n", "<CR>", function() self:click_button() end, keymap_opts)
    
    -- Store the keys we've set for cleanup
    self._keymap_keys = {
        "y", "Y", "a", "A", "n", "N", "<Left>", "<Right>", "<Tab>", "<S-Tab>", "<CR>"
    }
end

--- Clean up keymaps when widget is closed
function Confirm:_cleanup_keymaps()
    if not self._keymap_keys then return end
    
    for _, key in ipairs(self._keymap_keys) do
        pcall(vim.keymap.del, "n", key, { buffer = self.bufnr })
    end
end

--- Helper function for creating inline confirm dialogs
---@param opts {message: string, bufnr: number, line_nr: number, callback: fun(type: "yes"|"all"|"no", reason?: string)}
---@return string widget_id
function Confirm.create(opts)
    local confirm = Confirm:new(opts)
    return confirm:open()
end

return Confirm
