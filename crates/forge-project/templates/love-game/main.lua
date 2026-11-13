-- {{title}}: move the square with the arrow keys or WASD.

local player = { x = 400, y = 300, size = 32, speed = 240 }

function love.load()
  love.graphics.setBackgroundColor(0.08, 0.08, 0.1)
end

function love.update(dt)
  local dx, dy = 0, 0
  if love.keyboard.isDown("left", "a") then dx = dx - 1 end
  if love.keyboard.isDown("right", "d") then dx = dx + 1 end
  if love.keyboard.isDown("up", "w") then dy = dy - 1 end
  if love.keyboard.isDown("down", "s") then dy = dy + 1 end
  player.x = player.x + dx * player.speed * dt
  player.y = player.y + dy * player.speed * dt
end

function love.draw()
  love.graphics.setColor(1, 0.48, 0.1)
  love.graphics.rectangle("fill", player.x - player.size / 2, player.y - player.size / 2, player.size, player.size)
  love.graphics.setColor(1, 1, 1)
  love.graphics.print("{{title}}", 16, 16)
end

function love.keypressed(key)
  if key == "escape" then
    love.event.quit()
  end
end
