function move (x, y)
   return string.format("move(%f, %f)", x, y)
end

function random_move ()
   return move(math.random(400), math.random(400))
end
