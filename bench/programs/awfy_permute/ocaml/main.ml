(* Permute from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/permute.js;
   Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's
   typed ports: the benchmark object's `count` and the six-element array `v` are mutable fields,
   the array allocated per benchmark(); swaps write it in place. run(n) sums n benchmark()
   results. *)

type permute = { mutable count : int; mutable v : int array }

let swap p i j =
  let tmp = p.v.(i) in
  p.v.(i) <- p.v.(j);
  p.v.(j) <- tmp

let rec permute p n =
  p.count <- p.count + 1;
  if n <> 0 then begin
    let n1 = n - 1 in
    permute p n1;
    for i = n1 downto 0 do
      swap p n1 i;
      permute p n1;
      swap p n1 i
    done
  end

let benchmark p =
  p.count <- 0;
  p.v <- Array.make 6 0;
  permute p 6;
  p.count

let run n =
  let bench = { count = 0; v = [||] } in
  let sum = ref 0 in
  for _ = 1 to n do
    sum := !sum + benchmark bench
  done;
  !sum

let () = Runner.main run 4000
