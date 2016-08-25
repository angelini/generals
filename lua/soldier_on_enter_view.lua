function is_shooting(state)
   local prefix = string.sub(state, 1, string.len("shoot"))
   return prefix == "shoot"
end

if self["team"] ~= other["team"] and other["role"] == "soldier" then
   if not is_shooting(self["state"]) then
      self["state"] = string.format("shoot(%s)", other["id"])
   end
end
