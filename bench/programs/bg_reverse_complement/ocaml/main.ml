(* reverse-complement, the Benchmarks Game's task: read FASTA line by line and, for each
   sequence, write its header line and its reverse complement in 60-character lines (the IUB
   complement table; lower-case input gets the upper-case complement). The input is `fasta n`'s
   output, generated in memory by bg_fasta's code (copied below) as part of the timed work. A
   sequence's lines are gathered into a growing Buffer; the whole output is built in one Buffer,
   then digested (run) or written raw (`main output n`). *)

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

(* Each byte's complement: the IUB codes and their lower-case forms map to the upper-case
   complement, every other byte to itself. *)
let complements () =
  let table = Bytes.init 256 Char.chr in
  String.iteri
    (fun i f ->
      let t = "TGCAAKYWSRMBDHVN".[i] in
      Bytes.set table (Char.code f) t;
      Bytes.set table (Char.code f + 32) t)
    "ACGTUMRWSYKVHDBN";
  table

(* The reverse complement of a sequence in 60-character lines. *)
let reverse_complement out seq table =
  let i = ref (Buffer.length seq) in
  while !i > 0 do
    let line = min width !i in
    for _ = 1 to line do
      decr i;
      Buffer.add_char out (Bytes.get table (Char.code (Buffer.nth seq !i)))
    done;
    Buffer.add_char out '\n'
  done

(* What the Benchmarks Game program writes for `fasta n` on its input. *)
let revcomp n =
  let table = complements () in
  let out = Buffer.create 1024 and seq = Buffer.create 64 in
  List.iter
    (fun line ->
      if String.length line > 0 && line.[0] = '>' then begin
        reverse_complement out seq table;
        Buffer.add_string out line;
        Buffer.add_char out '\n';
        Buffer.clear seq
      end
      else Buffer.add_string seq line)
    (String.split_on_char '\n' (fasta n));
  reverse_complement out seq table;
  Buffer.contents out

let run n =
  let out = revcomp n in
  let h = ref 0 in
  String.iter (fun c -> h := (!h * 31 + Char.code c) mod 1000000007) out;
  !h

let () =
  if Array.length Sys.argv = 3 && Sys.argv.(1) = "output" then print_string (revcomp (int_of_string Sys.argv.(2)))
  else Runner.main run 25000000
