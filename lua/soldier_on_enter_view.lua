if self["team"] ~= other["team"] and other["role"] == "soldier" then
   self["state"] = string.format("shoot(%s)", other["id"])
end
