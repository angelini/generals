function new_soldier (id, x, y, team)
   return string.format("new_unit(soldier, %s, %f, %f, %d)", id, x, y, team)
end

function random_move_by_id (id)
   return string.format("update_state(%s, %s)", id, random_move())
end

function flatten_timeline (timeline)
   local flat = {}
   local i = 1

   for time, deltas in pairs(timeline) do
      for _, delta in ipairs(deltas) do
         flat[i] = string.format("(%d, %s)", time, delta)
         i = i + 1
      end
   end

   return flat
end

function timeline ()
   local team_1 = {}
   local team_2 = {}

   local deltas_at_0 = {}

   for i=1, 5 do
      local x = 75 * i
      local y = 50

      team_1[i] = uuid()
      deltas_at_0[i] = new_soldier(team_1[i], x, y, 1)

      team_2[i] = uuid()
      deltas_at_0[i + 5] = new_soldier(team_2[i], x, y + 300, 2)
   end

   local timeline = {
      [0] = deltas_at_0,
      [5] = {
         random_move_by_id(team_1[1]),
         random_move_by_id(team_2[1]),
         random_move_by_id(team_1[2]),
         random_move_by_id(team_2[2]),
         random_move_by_id(team_1[3]),
         random_move_by_id(team_2[3]),
         random_move_by_id(team_1[4]),
         random_move_by_id(team_2[4]),
         random_move_by_id(team_1[5]),
         random_move_by_id(team_2[5])
      }
   }

   return flatten_timeline(timeline)
end
