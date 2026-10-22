(* No deviation: self-tail sum and mutually tail-recursive parity, as in the Sakura source.
   OCaml ints are 63-bit and unchecked; the largest sum (500000500000) fits. *)
let rec sum_tail n acc = if n = 0 then acc else sum_tail (n - 1) (acc + n)

let rec is_even n = if n = 0 then true else is_odd (n - 1)
and is_odd n = if n = 0 then false else is_even (n - 1)

let run n =
  let parity = if is_even n then 1 else 0 in
  sum_tail n 0 + parity

let () = Runner.main run 1000000
