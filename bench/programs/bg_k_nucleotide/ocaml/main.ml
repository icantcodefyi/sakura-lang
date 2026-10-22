(* k-nucleotide, the Benchmarks Game's task: from FASTA input read line by line, extract
   sequence THREE (upper-cased), write the 1- and 2-nucleotide frequencies (%.3f, by descending
   frequency then ascending key) and the counts of GGT, GGTA, GGTATT, GGTATTTTAATT and
   GGTATTTTAATTTATAGT. `update` counts one reading frame into a stdlib Hashtbl (created small,
   it grows), holding an int ref per key; `count` combines all k frames. The input is `fasta n`'s
   output, generated in memory by bg_fasta's code (copied below) as part of the timed work. Keys
   are the nucleotides as codes 0..3 packed base 4, as in the Sakura program. *)

let alu =
  "GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGG" ^ "GAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGA"
  ^ "CCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAAT" ^ "ACAAAAATTAGCCGGGCGTGGTGGCGCGCGCCTGTAATCCCA"
  ^ "GCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGG" ^ "AGGCGGAGGTTGCAGTGAGCCGAGATCGCGCCACTGCACTCC"
  ^ "AGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA"

let iub =
  [| ('a', 0.27); ('c', 0.12); ('g', 0.12); ('t', 0.27); ('B', 0.02); ('D', 0.02); ('H', 0.02);
     ('K', 0.02); ('M', 0.02); ('N', 0.02); ('R', 0.02); ('S', 0.02); ('V', 0.02); ('W', 0.02);
     ('Y', 0.02) |]

let homo_sapiens =
  [| ('a', 0.3029549426680); ('c', 0.1979883004921); ('g', 0.1975473066391);
     ('t', 0.3015094502008) |]

let width = 60
let im = 139968
let ia = 3877
let ic = 29573

(* Cumulative probabilities, summed in table order. *)
let cumulative table =
  let p = ref 0.0 in
  Array.map (fun (c, q) -> p := !p +. q; (c, !p)) table

let repeat_fasta out header s count =
  Buffer.add_string out header;
  let doubled = s ^ s and len = String.length s in
  let pos = ref 0 and count = ref count in
  while !count > 0 do
    let line = min width !count in
    Buffer.add_substring out doubled !pos line;
    Buffer.add_char out '\n';
    pos := !pos + line;
    if !pos >= len then pos := !pos - len;
    count := !count - line
  done

let random_fasta out header table count seed =
  Buffer.add_string out header;
  let cum = cumulative table in
  let n = Array.length cum in
  let last = fst cum.(n - 1) in
  let seed = ref seed and count = ref count in
  while !count > 0 do
    let line = min width !count in
    for _ = 1 to line do
      seed := (!seed * ia + ic) mod im;
      let r = float_of_int !seed /. float_of_int im in
      let c = ref last and j = ref 0 in
      while !j < n do
        let code, p = cum.(!j) in
        if r < p then (c := code; j := n) else incr j
      done;
      Buffer.add_char out !c
    done;
    Buffer.add_char out '\n';
    count := !count - line
  done;
  !seed

(* What the Benchmarks Game fasta program writes for n: this program's input. *)
let fasta n =
  let lines k = (k + width - 1) / width in
  let out = Buffer.create (80 + (10 * n) + lines (2 * n) + lines (3 * n) + lines (5 * n)) in
  repeat_fasta out ">ONE Homo sapiens alu\n" alu (2 * n);
  let seed = random_fasta out ">TWO IUB ambiguity codes\n" iub (3 * n) 42 in
  ignore (random_fasta out ">THREE Homo sapiens frequency\n" homo_sapiens (5 * n) seed);
  Buffer.contents out

(* A nucleotide's code 0..3: A, C, G, T. *)
let code = function 'A' -> 0 | 'C' -> 1 | 'G' -> 2 | _ -> 3

(* Sequence THREE, read line by line from the FASTA text, upper-cased, as codes 0..3. *)
let sequence_three input =
  let seq = Buffer.create 64 in
  let inside = ref false in
  List.iter
    (fun line ->
      if String.length line > 0 && line.[0] = '>' then
        inside := String.starts_with ~prefix:">THREE" line
      else if !inside then
        String.iter (fun c -> Buffer.add_char seq (Char.chr (code c))) (String.uppercase_ascii line))
    (String.split_on_char '\n' input);
  Buffer.contents seq

(* Count the k-nucleotides starting at frame, frame + k, frame + 2k, ... into the table. *)
let update table seq k frame =
  let i = ref frame in
  while !i + k <= String.length seq do
    let key = ref 0 in
    for j = 0 to k - 1 do
      key := (!key * 4) + Char.code seq.[!i + j]
    done;
    (match Hashtbl.find_opt table !key with
     | Some c -> incr c
     | None -> Hashtbl.add table !key (ref 1));
    i := !i + k
  done

(* Every k-nucleotide's count, all k reading frames combined. *)
let count seq k =
  let table = Hashtbl.create 16 in
  for frame = 0 to k - 1 do
    update table seq k frame
  done;
  table

let letters key k = String.init k (fun j -> "ACGT".[key / (1 lsl (2 * (k - 1 - j))) mod 4])

(* The k-nucleotides with their percentage frequencies, by descending frequency then key. *)
let frequencies out seq k =
  let entries = Hashtbl.fold (fun key c acc -> (key, !c) :: acc) (count seq k) [] in
  let total = List.fold_left (fun sum (_, c) -> sum + c) 0 entries in
  let sorted = List.sort (fun (k1, c1) (k2, c2) -> if c1 <> c2 then compare c2 c1 else compare k1 k2) entries in
  List.iter
    (fun (key, c) ->
      Buffer.add_string out
        (Printf.sprintf "%s %.3f\n" (letters key k) (100.0 *. float_of_int c /. float_of_int total)))
    sorted;
  Buffer.add_char out '\n'

(* What the Benchmarks Game program writes for `fasta n` on its input. *)
let text n =
  let seq = sequence_three (fasta n) in
  let out = Buffer.create 1024 in
  frequencies out seq 1;
  frequencies out seq 2;
  List.iter
    (fun s ->
      let key = String.fold_left (fun key c -> (key * 4) + code c) 0 s in
      let found = match Hashtbl.find_opt (count seq (String.length s)) key with Some c -> !c | None -> 0 in
      Buffer.add_string out (Printf.sprintf "%d\t%s\n" found s))
    [ "GGT"; "GGTA"; "GGTATT"; "GGTATTTTAATT"; "GGTATTTTAATTTATAGT" ];
  Buffer.contents out

let run n =
  let out = text n in
  let h = ref 0 in
  String.iter (fun c -> h := (!h * 31 + Char.code c) mod 1000000007) out;
  !h

let () =
  if Array.length Sys.argv = 3 && Sys.argv.(1) = "output" then print_string (text (int_of_string Sys.argv.(2)))
  else Runner.main run 25000000
