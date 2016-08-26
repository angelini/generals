function general_on_state_change (self)
   if self["state"] == "idle" then
      self["state"] = random_move()
   end
end

function general_on_collision (self, other)
   if other["role"] == "bullet" then
      self["state"] = "dead"
   end
end
