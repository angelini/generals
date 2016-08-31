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
