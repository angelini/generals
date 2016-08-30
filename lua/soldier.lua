function is_shooting(state)
   local prefix = string.sub(state, 1, string.len("shoot"))
   return prefix == "shoot"
end

function soldier_on_collision (self, other)
   if other["role"] == "bullet" then
      return "dead"
   end
end

function soldier_on_enter_view (self, other)
   if self["team"] ~= other["team"] and other["role"] == "soldier" then
      if not is_shooting(self["state"]) then
         return string.format("shoot(%s)", other["id"])
      end
   end
end
