(* No deviation: three states that tail-call both others, as in the Sakura source. *)
let rec a n acc =
  if n = 0 then acc else if n mod 2 = 0 then b (n - 1) (acc + 1) else c (n - 1) (acc + 2)

and b n acc =
  if n = 0 then acc else if n mod 3 = 0 then a (n - 1) (acc + 3) else c (n - 1) (acc + 4)

and c n acc =
  if n = 0 then acc else if n mod 5 = 0 then a (n - 1) (acc + 5) else b (n - 1) (acc + 6)

let run n = a n 0

let () = Runner.main run 10000000
