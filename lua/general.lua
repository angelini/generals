function general_on_state_change (self)
   if self["state"] == "idle" then
      return random_move()
   end
end

function general_on_collision (self, other)
   if other["role"] == "bullet" then
      return "dead"
   end
end
