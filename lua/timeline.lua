function timeline ()
   local deltas_at_0 = {}
   local deltas_at_1 = {}
   local deltas_at_2 = {}

   for i=1, 5 do
      local x = 150 * i
      local y = 50

      if i % 2 == 0 then
         y = y + 100
      end

      id = uuid()
      deltas_at_0[i] = new_soldier(id, x, y, 1.57, 1)
      deltas_at_1[i] = update_state(id, move_to_random())
   end

   for i=6, 10 do
      local x = 150 * (i - 5)
      local y = 750

      if i % 2 == 0 then
         y = y - 100
      end


      id = uuid()
      deltas_at_0[i] = new_soldier(id, x, y, 4.71, 2)
      deltas_at_2[i - 5] = update_state(id, move_to_random())
   end

   local id = uuid()
   deltas_at_0[11] = new_general(id, 400, 400, 0, 1)
   deltas_at_0[12] = update_state(id, move_to_random())

   -- return {
   --    [0] = deltas_at_0,
   --    [1] = deltas_at_1,
   --    [2] = deltas_at_2
   -- }

   return {
      [0] = deltas_at_0
   }
end
