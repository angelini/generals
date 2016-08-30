function bullet_on_state_change (self)
   if self["state"] == "idle" then
      return "dead"
   end
end

function bullet_on_collision (self, other)
   return "dead"
end
