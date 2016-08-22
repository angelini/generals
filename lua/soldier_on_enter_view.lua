if self["team"] ~= other["team"] and other["role"] == "soldier" then
   self["state"] = string.format("shoot(%f, %f)", other["x"], other["y"])
end
