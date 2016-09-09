math.randomseed(os.time())

function move (x, y)
   return string.format("move(%f, %f)", x, y)
end

function move_to_random ()
   return move(math.random(400), math.random(400))
end

function command (id, state)
   return string.format("command(%s, %state)", id, state)
end

function shoot (id)
   return string.format("shoot(%s)", id)
end

function is_shooting(state)
   local prefix = string.sub(state, 1, string.len("shoot"))
   return prefix == "shoot"
end

function new_soldier (id, x, y, rotation, team)
   return string.format("new_unit(soldier, %s, %f, %f, %f, %d)", id, x, y, rotation, team)
end

function new_general (id, x, y, rotation, team)
   return string.format("new_unit(general, %s, %f, %f, %f, %d)", id, x, y, rotation, team)
end

function update_state (id, state)
   return string.format("update_state(%s, %s)", id, state)
end

function __flatten_timeline (timeline)
   local flat = {}
   local i = 1

   for time, deltas in pairs(timeline) do
      for _, delta in ipairs(deltas) do
         flat[i] = string.format("(%d, %s)", time, delta)
         i = i + 1
      end
   end

   return flat
end
