if self["state"] == "idle" and self["x"] ~= 200 then
   self["state"] = "moving(200.0, 200.0)"
end
