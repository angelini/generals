function bullet_on_state_change (self)
   if self["state"] == "idle" then
      self["state"] = "dead"
   end
end

function bullet_on_collision (self, other)
   self["state"] = "dead"
end
