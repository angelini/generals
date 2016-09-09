function timeline ()
   local units = {}
   local move_team_1 = {}
   local move_team_2 = {}

   for i=1, 5 do
      local x = 150 * i
      local y = 50

      if i % 2 == 0 then
         y = y + 100
      end

      id = uuid()
      units[i] = new_soldier(id, x, y, 1.57, 1)
      move_team_1[i] = update_state(id, move_to_random())
   end

   for i=6, 10 do
      local x = 150 * (i - 5)
      local y = 750

      if i % 2 == 0 then
         y = y - 100
      end


      id = uuid()
      units[i] = new_soldier(id, x, y, 4.71, 2)
      move_team_2[i - 5] = update_state(id, move_to_random())
   end

   local id = uuid()
   units[11] = new_general(id, 400, 400, 4.71, 1)
   move_general_right = {update_state(id, move(750, 80))}
   move_general_left = {update_state(id, move(50, 80))}

   return {
      [0] = units,
      [1] = move_general_right,
      [8] = move_general_left
   }
end
