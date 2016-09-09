function general_on_state_change (self)
   if self["state"] == "idle" then
      return move_to_random()
   end
end

function general_on_collision (self, other)
   if other["role"] == "bullet" then
      return "dead"
   end
end

function general_on_enter_view (self, other)
   if self["team"] == other["team"] and
      other["role"] == "soldier" and
      other["state"] == "idle" then
      return command(other["id"], move_to_random())
   end
end
