(* Storage from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/storage.js
   and som.js's Random; Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM
   benchmarks). Follows AWFY's typed ports: an array is an OCaml array of nodes, a leaf array's
   k slots holding Empty (AWFY's `new Array(k)`); the benchmark's `count` and Random's seed are
   mutable fields. Random keeps OCaml's `land` (Sakura takes `mod 65536`, the same value). run(n)
   sums n benchmark() results. *)

type random = { mutable seed : int }

let next random =
  random.seed <- ((random.seed * 1309) + 13849) land 65535;
  random.seed

type node = Empty | Array of node array

type storage = { mutable count : int }

let rec build_tree_depth s depth random =
  s.count <- s.count + 1;
  if depth = 1 then Array.make ((next random mod 10) + 1) Empty
  else begin
    let arr = Array.make 4 Empty in
    for i = 0 to 3 do
      arr.(i) <- Array (build_tree_depth s (depth - 1) random)
    done;
    arr
  end

let benchmark s =
  let random = { seed = 74755 } in
  s.count <- 0;
  ignore (Sys.opaque_identity (build_tree_depth s 7 random));
  s.count

let run n =
  let bench = { count = 0 } in
  let sum = ref 0 in
  for _ = 1 to n do
    sum := !sum + benchmark bench
  done;
  !sum

let () = Runner.main run 2000
