local api = vim.api
local fn = vim.fn
local Line = require("avante.ui.line")
local Utils = require("avante.utils")
local Highlights = require("avante.highlights")

---@class avante.ui.InlineWidgets
local M = {}

-- Namespace for inline widgets
M.NS = api.nvim_create_namespace("avante_inline_widgets")

-- Track active widgets to prevent multiple instances
M.active_widgets = {}

---@class InlineWidgetOptions
---@field id string Unique ID for this widget
---@field bufnr integer Buffer number
---@field line_nr integer Line number (0-indexed) where to insert the widget
---@field after_line boolean Whether to insert after the specified line
---@field delete_on_leave boolean Whether to delete the widget when cursor leaves
---@field priority? integer Display priority
---@field callback? function Function to call when widget is closed or action is taken

--- Base function to create an inline widget
---@param opts InlineWidgetOptions
---@param content string[] Content lines to display
---@param highlights? table<integer, {col_start: integer, col_end: integer, hl_group: string}[]> Highlights to apply
---@return integer[] extmark_ids
function M.create_widget(opts, content, highlights)
    -- Validate required options
    if not opts.id or not opts.bufnr or not api.nvim_buf_is_valid(opts.bufnr) then
        return {}
    end

    -- Clean up any existing widget with the same ID
    M.clear_widget(opts.id)

    local line_nr = opts.line_nr
    if opts.after_line then
        line_nr = line_nr + 1
    end

    -- Add virt lines to create the widget
    local extmark_id = api.nvim_buf_set_extmark(opts.bufnr, M.NS, line_nr, 0, {
        virt_lines = vim.tbl_map(function(line)
            return {{line, "Normal"}}
        end, content),
        virt_lines_above = false,
        priority = opts.priority or 100,
    })

    -- Register the widget so we can clear it later
    M.active_widgets[opts.id] = {
        bufnr = opts.bufnr,
        extmark_ids = {extmark_id},
        delete_on_leave = opts.delete_on_leave,
        callback = opts.callback
    }

    -- If we need to delete on cursor leave
    if opts.delete_on_leave then
        local augroup = api.nvim_create_augroup("AvanteInlineWidget_" .. opts.id, { clear = true })
        api.nvim_create_autocmd("CursorMoved", {
            group = augroup,
            buffer = opts.bufnr,
            callback = function()
                local cursor_pos = api.nvim_win_get_cursor(0)
                local cursor_line = cursor_pos[1] - 1
                
                -- If cursor is far enough from widget, delete it
                if math.abs(cursor_line - line_nr) > #content + 3 then
                    M.clear_widget(opts.id)
                    api.nvim_del_augroup_by_id(augroup)
                end
            end
        })
    end
    
    return {extmark_id}
end

--- Clear a specific widget by ID
---@param id string Widget ID to clear
function M.clear_widget(id)
    local widget = M.active_widgets[id]
    if widget then
        for _, extmark_id in ipairs(widget.extmark_ids) do
            pcall(api.nvim_buf_del_extmark, widget.bufnr, M.NS, extmark_id)
        end
        M.active_widgets[id] = nil
    end
end

--- Clear all widgets
function M.clear_all_widgets()
    for id, _ in pairs(M.active_widgets) do
        M.clear_widget(id)
    end
end

--- Create a confirmation dialog inline
---@param opts {message: string, bufnr: integer, line_nr: integer, callback: fun(type: "yes"|"all"|"no", reason?:string)}
---@return string widget_id
function M.confirm(opts)
    local message = opts.message
    local bufnr = opts.bufnr
    local line_nr = opts.line_nr
    local callback = opts.callback
    
    local widget_id = "confirm_" .. tostring(bufnr) .. "_" .. tostring(line_nr)
    
    -- Create the confirmation message content
    local content = {}
    
    -- Add empty line for spacing
    table.insert(content, "")
    
    -- Add message lines
    for _, line in ipairs(vim.split(message, "\n")) do
        table.insert(content, line)
    end

    -- Add buttons line
    local buttons_line = "   [Y]es    [A]ll yes    [N]o"
    table.insert(content, "")
    table.insert(content, buttons_line)
    table.insert(content, "")
    
    -- Create widget
    M.create_widget({
        id = widget_id,
        bufnr = bufnr,
        line_nr = line_nr,
        after_line = true,
        delete_on_leave = false,
        callback = callback,
    }, content)
    
    -- Add keymaps for this buffer
    local function handle_yes()
        callback("yes")
        M.clear_widget(widget_id)
    end
    
    local function handle_all()
        callback("all")
        M.clear_widget(widget_id)
    end
    
    local function handle_no()
        callback("no")
        M.clear_widget(widget_id)
    end

    -- Create a unique group for keymaps
    local keymap_group = "avante_confirm_" .. widget_id
    local augroup = api.nvim_create_augroup(keymap_group, { clear = true })
    
    -- Setup one-time keymaps for this buffer that self-clean
    vim.keymap.set("n", "y", handle_yes, { buffer = bufnr, nowait = true })
    vim.keymap.set("n", "Y", handle_yes, { buffer = bufnr, nowait = true })
    vim.keymap.set("n", "a", handle_all, { buffer = bufnr, nowait = true })
    vim.keymap.set("n", "A", handle_all, { buffer = bufnr, nowait = true })
    vim.keymap.set("n", "n", handle_no, { buffer = bufnr, nowait = true })
    vim.keymap.set("n", "N", handle_no, { buffer = bufnr, nowait = true })
    
    -- Create autocmd to clear keymaps when widget is cleared
    api.nvim_create_autocmd("User", {
        group = augroup,
        pattern = "AvanteWidgetCleared_" .. widget_id,
        callback = function()
            vim.keymap.del("n", "y", { buffer = bufnr })
            vim.keymap.del("n", "Y", { buffer = bufnr })
            vim.keymap.del("n", "a", { buffer = bufnr })
            vim.keymap.del("n", "A", { buffer = bufnr })
            vim.keymap.del("n", "n", { buffer = bufnr })
            vim.keymap.del("n", "N", { buffer = bufnr })
            api.nvim_del_augroup_by_name(keymap_group)
        end,
    })
    
    -- When widget is cleared, trigger event to clean up keymaps
    local old_clear_widget = M.clear_widget
    M.clear_widget = function(id)
        old_clear_widget(id)
        api.nvim_exec_autocmds("User", { pattern = "AvanteWidgetCleared_" .. id })
    end
    
    return widget_id
end

--- Create an input prompt inline
---@param opts {bufnr: integer, line_nr: integer, prompt?: string, default_value?: string, callback: fun(input: string)}
---@return string widget_id
function M.prompt_input(opts)
    local bufnr = opts.bufnr
    local line_nr = opts.line_nr
    local prompt = opts.prompt or "Enter input:"
    local default_value = opts.default_value or ""
    local callback = opts.callback
    
    local widget_id = "prompt_input_" .. tostring(bufnr) .. "_" .. tostring(line_nr)
    
    -- Create a temporary buffer for the input
    local input_bufnr = api.nvim_create_buf(false, true)
    api.nvim_buf_set_option(input_bufnr, "buftype", "prompt")
    api.nvim_buf_set_option(input_bufnr, "bufhidden", "wipe")
    
    -- Set the prompt text
    api.nvim_buf_set_option(input_bufnr, "prompt", prompt .. " ")
    
    -- Set default value if provided
    if default_value ~= "" then
        api.nvim_buf_set_lines(input_bufnr, 0, -1, false, {default_value})
    end
    
    -- Create floating window for input
    local width = math.min(60, api.nvim_win_get_width(0) - 4)
    local height = 1
    
    local win_opts = {
        relative = 'win',
        width = width,
        height = height,
        row = line_nr + 1,
        col = 2,
        style = 'minimal',
        border = 'single',
    }
    
    local winid = api.nvim_open_win(input_bufnr, true, win_opts)
    
    -- Register the input widget
    M.active_widgets[widget_id] = {
        bufnr = bufnr,
        extmark_ids = {},
        input_bufnr = input_bufnr,
        input_winid = winid,
        delete_on_leave = false,
        callback = callback
    }
    
    -- Start in insert mode
    vim.cmd('startinsert')
    
    -- Setup keymaps for submit/cancel
    vim.keymap.set('i', '<CR>', function()
        local input = api.nvim_buf_get_lines(input_bufnr, 0, -1, false)[1] or ""
        callback(input)
        M.clear_widget(widget_id)
    end, {buffer = input_bufnr, nowait = true})
    
    vim.keymap.set('i', '<Esc>', function()
        M.clear_widget(widget_id)
    end, {buffer = input_bufnr, nowait = true})
    
    -- Override clear_widget to properly clean up the input window
    local old_clear_widget = M.clear_widget
    M.clear_widget = function(id)
        local widget = M.active_widgets[id]
        if widget and widget.input_winid and api.nvim_win_is_valid(widget.input_winid) then
            api.nvim_win_close(widget.input_winid, true)
        end
        old_clear_widget(id)
    end
    
    return widget_id
end

return M
