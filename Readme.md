An early implementation of a game engine in Rust with Lua scriptable entities.

Script soldiers to shoot soldiers from the other team, when they come into view.

```lua
function soldier_on_state_change (self)
   if self["state"] == "idle" then
      return move_to_random()
   end
end

function soldier_on_collision (self, other)
   if other["role"] == "bullet" then
      return "dead"
   end
end

function soldier_on_enter_view (self, other)
   if self["team"] ~= other["team"] and other["role"] == "soldier" then
      if not is_shooting(self["state"]) then
         return shoot(other["id"])
      end
   end
end
```

Set up a timeline where all soldiers appear at time 0, at time 2 all of team 1 starts moving in random directions and at time 4 all of team 2 starts doing the same.

```lua
function timeline ()
   local deltas_at_0 = {}
   local deltas_at_2 = {}
   local deltas_at_4 = {}

   for i=1, 5 do
      local x = 150 * i
      local y = 50

      id = uuid()
      deltas_at_0[i] = new_soldier(id, x, y, 3.1415, 1)
      deltas_at_2[i] = update_state(id, move_to_random())
   end

   for i=6, 10 do
      local x = 150 * (i - 5)
      local y = 750

      id = uuid()
      deltas_at_0[i] = new_soldier(id, x, y, 0, 2)
      deltas_at_4[i - 5] = update_state(id, move_to_random())
   end

   return {
      [0] = deltas_at_0,
      [2] = deltas_at_2,
      [4] = deltas_at_4
   }
end
```

And you get

![demo](./demo.gif)
