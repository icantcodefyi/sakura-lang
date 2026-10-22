(* List from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/list.js;
   Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). An element is a
   record whose `next` is an option (None for null; set after allocation, as AWFY's makeList
   does, so it is mutable); AWFY's `x.next` on a list known to be non-empty is `next x`, which
   raises on None where JavaScript would throw. isShorterThan's early return is a finished flag
   on the while loop (OCaml has no return). run(n) sums n benchmark() results. *)

type element = { value : int; mutable next : element option }

let rec length e =
  match e.next with
  | None -> 1
  | Some n -> 1 + length n

let next = function
  | Some e -> e.next
  | None -> invalid_arg "element"

let rec make_list len =
  if len = 0 then None
  else begin
    let e = { value = len; next = None } in
    e.next <- make_list (len - 1);
    Some e
  end

let is_shorter_than x y =
  let x_tail = ref x in
  let y_tail = ref y in
  let result = ref false in
  let finished = ref false in
  while (not !finished) && Option.is_some !y_tail do
    if Option.is_none !x_tail then begin
      result := true;
      finished := true
    end
    else begin
      x_tail := next !x_tail;
      y_tail := next !y_tail
    end
  done;
  !result

let rec tail x y z =
  if is_shorter_than y x then tail (tail (next x) y z) (tail (next y) z x) (tail (next z) x y)
  else z

let benchmark () =
  match tail (make_list 15) (make_list 10) (make_list 6) with
  | Some result -> length result
  | None -> invalid_arg "result"

let run n =
  let sum = ref 0 in
  for _ = 1 to n do
    sum := !sum + benchmark ()
  done;
  !sum

let () = Runner.main run 6000
