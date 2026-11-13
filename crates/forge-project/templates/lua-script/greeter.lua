--- Greeting helpers for {{title}}.
local M = {}

---@param name string
---@return string
function M.greet(name)
  return ("Hello, %s!"):format(name)
end

return M
