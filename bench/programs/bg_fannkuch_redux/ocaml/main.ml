(* fannkuch-redux, the Benchmarks Game's task: the task description's sequential algorithm
   (Oleg Mazurov's count[] order, as the task page credits him) over int
   arrays updated in place, where the Sakura program updates persistent Lists; same permutation
   order, flips and checksum. The Java program's `break` and early `return` are the exception
   `Done` and a `next` flag. For n = 0 the checksum and maximum are 0, as in the Sakura program.
   `main output <n>` writes the program's output; run(n) is the digest of the same bytes
   (bench/README.md). *)
exception Done

(* (checksum, maximum flips) over all n! permutations. *)
let fannkuch n =
  if n = 0 then (0, 0)
  else begin
    let perm = Array.make n 0 and perm1 = Array.init n Fun.id and count = Array.make n 0 in
    let max_flips = ref 0 and perm_count = ref 0 and checksum = ref 0 in
    let r = ref n in
    (try
       while true do
         while !r <> 1 do
           count.(!r - 1) <- !r;
           decr r
         done;
         Array.blit perm1 0 perm 0 n;
         let flips = ref 0 in
         while perm.(0) <> 0 do
           let k = perm.(0) in
           let k2 = (k + 1) lsr 1 in
           for i = 0 to k2 - 1 do
             let t = perm.(i) in
             perm.(i) <- perm.(k - i);
             perm.(k - i) <- t
           done;
           incr flips
         done;
         max_flips := max !max_flips !flips;
         checksum := if !perm_count mod 2 = 0 then !checksum + !flips else !checksum - !flips;
         let next = ref false in
         while not !next do
           if !r = n then raise Done;
           let perm0 = perm1.(0) in
           for i = 0 to !r - 1 do perm1.(i) <- perm1.(i + 1) done;
           perm1.(!r) <- perm0;
           count.(!r) <- count.(!r) - 1;
           if count.(!r) > 0 then next := true else incr r
         done;
         incr perm_count
       done
     with Done -> ());
    (!checksum, !max_flips)
  end

(* What the Benchmarks Game program writes for n. *)
let text n =
  let checksum, max_flips = fannkuch n in
  Printf.sprintf "%d\nPfannkuchen(%d) = %d\n" checksum n max_flips

(* h = (31·h + byte) mod 1,000,000,007 over every byte. *)
let digest s = String.fold_left (fun h c -> (h * 31 + Char.code c) mod 1000000007) 0 s

let maximum = 12

let () =
  if Array.length Sys.argv = 3 && Sys.argv.(1) = "output" then begin
    let n = int_of_string Sys.argv.(2) in
    if n < 0 || n > maximum then exit 2;
    print_string (text n)
  end else Runner.main (fun n -> digest (text n)) maximum
