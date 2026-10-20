/**
 * A small LÖVE 2D game for browser mode, opened with `?project=lua` (the default demo is the
 * Laravel shop in demoProject.ts). Its files are indexed by the Rust Lua index through
 * WebAssembly (luaMock.ts).
 */
import type { ProjectInfo } from "@/ipc/project";

export const luaDemoName = "pixel-dungeon";

/** True when the page asks for the Lua demo project. */
export const isLuaDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "lua";

export const luaDemoInfo: ProjectInfo = {
  languages: ["lua"],
  frameworks: [{ id: "love", name: "LÖVE", version: "11.5", language: "lua" }],
  packs: ["luaforge"],
  tools: ["git", "luarocks"],
  primary: { id: "love", name: "LÖVE", version: "11.5", language: "lua" },
};

export const luaDemoFiles: Record<string, string> = {
  "main.lua": `--- Pixel Dungeon: a tiny roguelike made with LÖVE.
local Player = require("src.player")
local World = require("src.world")
local Hud = require("src.ui.hud")
local Vec2 = require("lib.vector")

---@type Player
local player
---@type World
local world

local SPAWN = Vec2.new(4, 3)

function love.load()
  love.graphics.setDefaultFilter("nearest", "nearest")
  world = World.generate(48, 32, os.time())
  player = Player.new("Ada", SPAWN)
  Hud.attach(player)
end

---@param dt number Seconds since the last frame
function love.update(dt)
  local dir = Vec2.new(0, 0)
  if love.keyboard.isDown("left") then dir.x = -1 end
  if love.keyboard.isDown("right") then dir.x = 1 end
  if love.keyboard.isDown("up") then dir.y = -1 end
  if love.keyboard.isDown("down") then dir.y = 1 end
  if dir:len() > 0 and world:isWalkable(player.pos + dir) then
    player:move(dir, dt)
  end
  world:update(dt)
end

function love.draw()
  world:draw()
  player:draw()
  Hud.draw()
end

function love.keypressed(key)
  if key == "escape" then
    love.event.quit()
  elseif key == "f3" then
    DEBUG = not DEBUG
  end
end
`,
  "conf.lua": `--- Draw hit boxes and the frame rate (also switched on with F3 in main.lua).
DEBUG = false

function love.conf(t)
  t.identity = "pixel-dungeon"
  t.version = "11.5"
  t.window.title = "Pixel Dungeon"
  t.window.width = 960
  t.window.height = 640
  t.window.vsync = 1
end
`,
  "src/entity.lua": `---@class Entity
---@field pos Vec2 Position in tiles
---@field hp integer Hit points; the entity dies at 0
---@field sprite love.Quad?
local Entity = {}
Entity.__index = Entity

--- Creates an entity at a tile position.
---@param pos Vec2
---@param hp? integer Defaults to 10
---@return Entity
function Entity.new(pos, hp)
  return setmetatable({ pos = pos, hp = hp or 10 }, Entity)
end

--- Whether the entity still has hit points left.
---@return boolean
function Entity:isAlive()
  return self.hp > 0
end

--- Applies damage and returns the remaining hit points.
---@param amount integer
---@return integer hp # remaining hit points
function Entity:damage(amount)
  self.hp = math.max(0, self.hp - amount)
  return self.hp
end

return Entity
`,
  "src/player.lua": `local Entity = require("src.entity")
local Vec2 = require("lib.vector")

--- The hero, controlled with the arrow keys.
---@class Player : Entity
---@field name string Display name shown in the HUD
---@field speed number Tiles per second
---@field gold integer
local Player = setmetatable({}, { __index = Entity })
Player.__index = Player

local SPEED = 6

--- Creates the player.
---@param name string Display name
---@param pos Vec2 Spawn position in tiles
---@return Player
function Player.new(name, pos)
  local self = setmetatable(Entity.new(pos, 20), Player)
  self.name = name
  self.speed = SPEED
  self.gold = 0
  return self
end

--- Moves the player by a direction, scaled by speed and frame time.
---
--- Collisions are checked by the caller (\`World:isWalkable\`).
---@param dir Vec2 Unit direction
---@param dt number Frame time in seconds
---@return Vec2 pos # the new position
function Player:move(dir, dt)
  self.pos = self.pos + dir:scale(self.speed * dt)
  return self.pos
end

---@param amount integer
function Player:pickUpGold(amount)
  self.gold = self.gold + amount
end

function Player:draw()
  love.graphics.setColor(1, 0.8, 0.3)
  love.graphics.rectangle("fill", self.pos.x * 16, self.pos.y * 16, 16, 16)
end

return Player
`,
  "src/world.lua": `local Vec2 = require("lib.vector")

---@alias Tile
---| "floor" # Walkable
---| "wall"
---| "water" # Slows down

---@class World
---@field width integer
---@field height integer
---@field tiles Tile[][]
local World = {}
World.__index = World

--- Generates a dungeon with rooms and corridors.
---@param width integer
---@param height integer
---@param seed? integer Random seed, \`os.time()\` by default
---@return World
function World.generate(width, height, seed)
  math.randomseed(seed or os.time())
  local self = setmetatable({ width = width, height = height, tiles = {} }, World)
  for y = 1, height do
    self.tiles[y] = {}
    for x = 1, width do
      self.tiles[y][x] = math.random() < 0.1 and "wall" or "floor"
    end
  end
  return self
end

---@param pos Vec2
---@return boolean
function World:isWalkable(pos)
  local row = self.tiles[math.floor(pos.y)]
  return row ~= nil and row[math.floor(pos.x)] == "floor"
end

---@param dt number
function World:update(dt) end

function World:draw()
  for y, row in ipairs(self.tiles) do
    for x, tile in ipairs(row) do
      if tile == "wall" then
        love.graphics.rectangle("fill", x * 16, y * 16, 16, 16)
      end
    end
  end
end

return World
`,
  "src/ui/hud.lua": `--- Heads-up display: hit points and gold.
local Hud = {}

---@type Player?
local target

---@param player Player
function Hud.attach(player)
  target = player
end

function Hud.draw()
  if not target then return end
  love.graphics.setColor(1, 1, 1)
  love.graphics.print(("%s  HP %d  Gold %d"):format(target.name, target.hp, target.gold), 8, 8)
  if DEBUG then
    love.graphics.print(("FPS %d"):format(love.timer.getFPS()), 8, 24)
  end
end

return Hud
`,
  "lib/vector.lua": `--- 2D vectors with operators.
---@class Vec2
---@field x number
---@field y number
---@operator add(Vec2): Vec2
local Vec2 = {}
Vec2.__index = Vec2

--- Creates a vector.
---@param x number
---@param y number
---@return Vec2
function Vec2.new(x, y)
  return setmetatable({ x = x, y = y }, Vec2)
end

--- Length of the vector.
---@return number
function Vec2:len()
  return math.sqrt(self.x * self.x + self.y * self.y)
end

---@param factor number
---@return Vec2
function Vec2:scale(factor)
  return Vec2.new(self.x * factor, self.y * factor)
end

function Vec2.__add(a, b)
  return Vec2.new(a.x + b.x, a.y + b.y)
end

return Vec2
`,
  "tools/build.lua": `#!/usr/bin/env lua
-- Packs the game into dist/pixel-dungeon.love.
local files = { "main.lua", "conf.lua", "src", "lib", "assets" }

local function run(cmd)
  print("$ " .. cmd)
  assert(os.execute(cmd))
end

run("mkdir -p dist")
run("zip -9 -r dist/pixel-dungeon.love " .. table.concat(files, " "))
print("Built dist/pixel-dungeon.love")
`,
  ".luarc.json": `{
  "$schema": "https://raw.githubusercontent.com/LuaLS/vscode-lua/master/setting/schema.json",
  "runtime.version": "LuaJIT",
  "runtime.path": ["?.lua", "?/init.lua"],
  "workspace.library": ["\${3rd}/love2d/library"],
  "diagnostics.globals": ["love"]
}
`,
  "stylua.toml": `column_width = 120
indent_type = "Spaces"
indent_width = 2
quote_style = "AutoPreferDouble"
`,
  ".gitignore": `dist/
*.love
`,
  "README.md": `# Pixel Dungeon

A tiny roguelike made with [LÖVE](https://love2d.org) 11.5.

\`\`\`sh
love .              # run the game
lua tools/build.lua # pack dist/pixel-dungeon.love
\`\`\`
`,
};
