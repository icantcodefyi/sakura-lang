(* No deviation: the contracts program with its checks removed. *)
let increment x = x + 1

let total xs = List.fold_left ( + ) 0 xs

let run n =
  let xs = List.init 16 Fun.id in
  let i = ref 0 and acc = ref 0 in
  while !i < n do
    i := increment !i;
    if !i mod 16 = 0 then acc := !acc + total xs
  done;
  !acc + !i

let () = Runner.main run 1000000
