let main run maximum =
  if Array.length Sys.argv < 3 then invalid_arg "values <n>... or measure <n> [--stable]";
  let mode = Sys.argv.(1) in
  let args = Array.to_list (Array.sub Sys.argv 2 (Array.length Sys.argv - 2)) in
  let stable = List.mem "--stable" args in
  let sizes = List.map int_of_string (List.filter ((<>) "--stable") args) in
  if sizes = [] || List.exists (fun n -> n < 0 || n > maximum) sizes then
    invalid_arg "size outside oracle resource bound";
  if mode = "values" then (
    let values = List.map (fun n -> Printf.sprintf "\"%d\":\"%d\"" n (run n)) sizes in
    Printf.printf "{\"values\":{%s}}\n" (String.concat "," values)
  ) else match mode, sizes with
  | "measure", [n] ->
    let sink = ref 0 in
    let batch count = for _ = 1 to count do sink := Sys.opaque_identity (run (Sys.opaque_identity n)) done in
    let start = Unix.gettimeofday () in
    while Unix.gettimeofday () -. start < (if stable then 2. else 0.25) do batch 1 done;
    let count = ref 1 in
    let calibrated = ref false in
    while not !calibrated do
      let start = Unix.gettimeofday () in
      batch !count;
      if Unix.gettimeofday () -. start >= 0.03 || !count >= 1 lsl 20 then calibrated := true
      else count := !count * 2
    done;
    for _ = 1 to 9 do
      let start = Unix.gettimeofday () in
      batch !count;
      let ms = (Unix.gettimeofday () -. start) *. 1000. /. float_of_int !count in
      Printf.printf "{\"n\":%d,\"ms\":%.12g,\"checksum\":\"%d\"}\n" n ms !sink
    done
  | _ -> invalid_arg "measure <n> [--stable]"
