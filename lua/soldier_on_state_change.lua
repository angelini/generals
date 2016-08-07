if state == "idle" then
  state = string.format("moving(%f, %f)", math.random(350), math.random(350))
end
