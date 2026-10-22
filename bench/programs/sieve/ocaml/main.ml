(* Deviation: a mutable bool array written in place, like every control (Sakura uses List.set). *)
let run n =
  let flags = Array.init n (fun i -> i >= 2) in
  let p = ref 2 in
  while !p * !p < n do
    if flags.(!p) then begin
      let j = ref (!p * !p) in
      while !j < n do
        flags.(!j) <- false;
        j := !j + !p
      done
    end;
    incr p
  done;
  Array.fold_left (fun count f -> if f then count + 1 else count) 0 flags

let () = Runner.main run 200000
