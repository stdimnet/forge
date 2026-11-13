-- {{title}}

local greeter = require("lib.greeter")

local name = arg and arg[1] or "World"
print(greeter.greet(name))
