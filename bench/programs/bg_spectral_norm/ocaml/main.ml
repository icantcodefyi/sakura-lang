(* spectral-norm, the Benchmarks Game's task: the Java program's four functions (A, Av, Atv,
   AtAv), ten power iterations, the norm as a `%.9f` line. Deviation: float arrays built with
   Array.init (Sakura pushes onto persistent Lists); the Float operation order is the Sakura
   program's. For n = 0 the norm is taken as 0, as in the Sakura program. `main output <n>` writes
   the program's output; run(n) is the digest of the same bytes (bench/README.md). *)
let a i j = 1.0 /. float_of_int (((i + j) * (i + j + 1) / 2) + i + 1)

let mul_av v n =
  Array.init n (fun i ->
    let sum = ref 0.0 in
    for j = 0 to n - 1 do sum := !sum +. a i j *. v.(j) done;
    !sum)

let mul_atv v n =
  Array.init n (fun i ->
    let sum = ref 0.0 in
    for j = 0 to n - 1 do sum := !sum +. a j i *. v.(j) done;
    !sum)

let mul_atav v n = mul_atv (mul_av v n) n

let approximate n =
  if n = 0 then 0.0
  else begin
    let u = ref (Array.make n 1.0) in
    let v = ref !u in
    for _ = 1 to 10 do
      v := mul_atav !u n;
      u := mul_atav !v n
    done;
    let vbv = ref 0.0 and vv = ref 0.0 in
    for m = 0 to n - 1 do
      vbv := !vbv +. !u.(m) *. !v.(m);
      vv := !vv +. !v.(m) *. !v.(m)
    done;
    Float.sqrt (!vbv /. !vv)
  end

(* What the Benchmarks Game program writes for n. *)
let text n = Printf.sprintf "%.9f\n" (approximate n)

(* h = (31·h + byte) mod 1,000,000,007 over every byte. *)
let digest s = String.fold_left (fun h c -> (h * 31 + Char.code c) mod 1000000007) 0 s

let maximum = 5500

let () =
  if Array.length Sys.argv = 3 && Sys.argv.(1) = "output" then begin
    let n = int_of_string Sys.argv.(2) in
    if n < 0 || n > maximum then exit 2;
    print_string (text n)
  end else Runner.main (fun n -> digest (text n)) maximum
