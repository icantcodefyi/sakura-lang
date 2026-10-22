(* Deviation: OCaml's 63-bit int add is unchecked; fib(30) is far inside the bound. *)
let rec fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)

let () = Runner.main fib 30
