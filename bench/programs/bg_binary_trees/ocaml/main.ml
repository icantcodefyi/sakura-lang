(* binary-trees, the Benchmarks Game's task: the stretch tree, the long-lived tree and the
   bottom-up trees per depth, with the suite's report lines. Every node is a `Node` block, leaves
   included (`Node (Leaf, Leaf)`: `Leaf` is an immediate constant, the Java program's null
   child); OCaml's default GC. `main output <n>` writes the program's output; run(n) is the
   digest of the same bytes (bench/README.md). *)
type tree = Leaf | Node of tree * tree

let rec bottom_up d =
  if d > 0 then let l = bottom_up (d - 1) in let r = bottom_up (d - 1) in Node (l, r)
  else Node (Leaf, Leaf)

let rec check = function
  | Leaf -> 0
  | Node (l, r) -> let a = check l in let b = check r in 1 + a + b

(* What the Benchmarks Game program writes for n. *)
let text n =
  let min_depth = 4 in
  let max_depth = max (min_depth + 2) n in
  let stretch_depth = max_depth + 1 in
  let out = Buffer.create 1024 in
  Printf.bprintf out "stretch tree of depth %d\t check: %d\n" stretch_depth
    (check (bottom_up stretch_depth));
  let long_lived = bottom_up max_depth in
  let d = ref min_depth in
  while !d <= max_depth do
    let iterations = 1 lsl (max_depth - !d + min_depth) in
    let sum = ref 0 in
    for _ = 1 to iterations do sum := !sum + check (bottom_up !d) done;
    Printf.bprintf out "%d\t trees of depth %d\t check: %d\n" iterations !d !sum;
    d := !d + 2
  done;
  Printf.bprintf out "long lived tree of depth %d\t check: %d\n" max_depth (check long_lived);
  Buffer.contents out

(* h = (31·h + byte) mod 1,000,000,007 over every byte. *)
let digest s = String.fold_left (fun h c -> (h * 31 + Char.code c) mod 1000000007) 0 s

let maximum = 21

let () =
  if Array.length Sys.argv = 3 && Sys.argv.(1) = "output" then begin
    let n = int_of_string Sys.argv.(2) in
    if n < 0 || n > maximum then exit 2;
    print_string (text n)
  end else Runner.main (fun n -> digest (text n)) maximum
