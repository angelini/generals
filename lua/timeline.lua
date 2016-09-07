function timeline ()
   local deltas_at_0 = {}
   local deltas_at_2 = {}
   local deltas_at_4 = {}

   for i=1, 5 do
      local x = 150 * i
      local y = 50

      id = uuid()
      deltas_at_0[i] = new_soldier(id, x, y, 1.57, 1)
      deltas_at_2[i] = update_state(id, move_to_random())
   end

   for i=6, 10 do
      local x = 150 * (i - 5)
      local y = 750

      id = uuid()
      deltas_at_0[i] = new_soldier(id, x, y, 4.71, 2)
      deltas_at_4[i - 5] = update_state(id, move_to_random())
   end

   return {
      [0] = deltas_at_0,
      [2] = deltas_at_2,
      [4] = deltas_at_4
   }
end
