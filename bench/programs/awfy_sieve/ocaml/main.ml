(* Sieve from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/sieve.js;
   Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's
   typed ports: the 5000 flags are an array filled with true and written in place. run(n) sums n
   benchmark() results. *)

let sieve flags size =
  let prime_count = ref 0 in
  for i = 2 to size do
    if flags.(i - 1) then begin
      incr prime_count;
      let k = ref (i + i) in
      while !k <= size do
        flags.(!k - 1) <- false;
        k := !k + i
      done
    end
  done;
  !prime_count

let benchmark () =
  let flags = Array.make 5000 true in
  sieve flags 5000

let run n =
  let sum = ref 0 in
  for _ = 1 to n do
    sum := !sum + benchmark ()
  done;
  !sum

let () = Runner.main run 3000
