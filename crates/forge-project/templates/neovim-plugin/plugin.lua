if vim.g.loaded_{{module}} then
  return
end
vim.g.loaded_{{module}} = true

vim.api.nvim_create_user_command("{{class}}Greet", function(cmd)
  require("{{module}}").greet(cmd.args ~= "" and cmd.args or nil)
end, { nargs = "?", desc = "{{title}}: say hello" })
