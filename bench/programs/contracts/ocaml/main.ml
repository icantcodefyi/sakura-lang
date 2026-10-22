(* Deviation: requires/ensures are explicit tests that fail with the defect code (D020, D021),
   at the places the Rust port checks them. *)
let increment x =
  if not (x >= 0) then failwith "D020";
  let result = x + 1 in
  if not (result > x) then failwith "D021";
  result

let total xs =
  if not (List.for_all (fun x -> x >= 0) xs) then failwith "D020";
  List.fold_left ( + ) 0 xs

let run n =
  let xs = List.init 16 Fun.id in
  let i = ref 0 and acc = ref 0 in
  while !i < n do
    i := increment !i;
    if !i mod 16 = 0 then acc := !acc + total xs
  done;
  !acc + !i

let () = Runner.main run 1000000
