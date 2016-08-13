if self["state"] == "idle" then
   self["state"] = string.format("moving(%f, %f)", math.random(350), math.random(350))
end
