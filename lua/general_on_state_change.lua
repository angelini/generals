if self["state"] == "idle" and self["x"] ~= 0 then
   self["state"] = "move(0.0, 0.0)"
end
