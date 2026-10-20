--- {{title}}: a Neovim plugin.
local M = {}

---@class {{module}}.Config
---@field greeting string
M.config = {
  greeting = "Hello",
}

---@param opts? {{module}}.Config
function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", M.config, opts or {})
end

---@param name? string
function M.greet(name)
  vim.notify(("%s, %s!"):format(M.config.greeting, name or "Neovim"), vim.log.levels.INFO)
end

return M
