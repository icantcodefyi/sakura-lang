(* Queens from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/queens.js;
   Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's
   typed ports: the four arrays are mutable fields of the benchmark object, allocated and filled
   by every queens() call and written in place. placeQueen's early returns are a while loop with
   a found flag (OCaml has no return). run(n) sums n benchmark() results, true = 1. *)

type queens = {
  mutable free_maxs : bool array;
  mutable free_rows : bool array;
  mutable free_mins : bool array;
  mutable queen_rows : int array;
}

let get_row_column q r c = q.free_rows.(r) && q.free_maxs.(c + r) && q.free_mins.(c - r + 7)

let set_row_column q r c v =
  q.free_rows.(r) <- v;
  q.free_maxs.(c + r) <- v;
  q.free_mins.(c - r + 7) <- v

let rec place_queen q c =
  let found = ref false in
  let r = ref 0 in
  while (not !found) && !r < 8 do
    if get_row_column q !r c then begin
      q.queen_rows.(!r) <- c;
      set_row_column q !r c false;
      if c = 7 then found := true
      else if place_queen q (c + 1) then found := true
      else set_row_column q !r c true
    end;
    incr r
  done;
  !found

let queens q =
  q.free_rows <- Array.make 8 true;
  q.free_maxs <- Array.make 16 true;
  q.free_mins <- Array.make 16 true;
  q.queen_rows <- Array.make 8 (-1);
  place_queen q 0

let benchmark q =
  let result = ref true in
  for _ = 0 to 9 do
    result := !result && queens q
  done;
  !result

let run n =
  let bench = { free_maxs = [||]; free_rows = [||]; free_mins = [||]; queen_rows = [||] } in
  let sum = ref 0 in
  for _ = 1 to n do
    if benchmark bench then incr sum
  done;
  !sum

let () = Runner.main run 8000
