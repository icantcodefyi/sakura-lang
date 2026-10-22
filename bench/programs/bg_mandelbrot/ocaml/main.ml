(* mandelbrot, the Benchmarks Game's task: an n-by-n PBM P4 bitmap of [-1.5-i, 0.5+i], 50
   iterations, escape when |z|^2 > 4 (the reference C program, contributed by Greg Buchholz). The
   whole output is built in one Buffer, then digested (run) or written raw (`main output n`). *)

let bitmap n =
  let out = Buffer.create (32 + ((n + 7) / 8 * n)) in
  Buffer.add_string out (Printf.sprintf "P4\n%d %d\n" n n);
  let fn = float_of_int n in
  for y = 0 to n - 1 do
    let ci = 2.0 *. float_of_int y /. fn -. 1.0 in
    let acc = ref 0 and bits = ref 0 in
    for x = 0 to n - 1 do
      let cr = 2.0 *. float_of_int x /. fn -. 1.5 in
      let zr = ref 0.0 and zi = ref 0.0 and tr = ref 0.0 and ti = ref 0.0 in
      let i = ref 0 in
      while !i < 50 && !tr +. !ti <= 4.0 do
        zi := 2.0 *. !zr *. !zi +. ci;
        zr := !tr -. !ti +. cr;
        tr := !zr *. !zr;
        ti := !zi *. !zi;
        incr i
      done;
      acc := (!acc lsl 1) lor (if !tr +. !ti <= 4.0 then 1 else 0);
      incr bits;
      if !bits = 8 then begin
        Buffer.add_char out (Char.chr !acc);
        acc := 0;
        bits := 0
      end else if x = n - 1 then begin
        Buffer.add_char out (Char.chr ((!acc lsl (8 - n mod 8)) land 255));
        acc := 0;
        bits := 0
      end
    done
  done;
  Buffer.contents out

let run n =
  let out = bitmap n in
  let h = ref 0 in
  String.iter (fun c -> h := (!h * 31 + Char.code c) mod 1000000007) out;
  !h

let () =
  if Array.length Sys.argv = 3 && Sys.argv.(1) = "output" then
    print_string (bitmap (int_of_string Sys.argv.(2)))
  else Runner.main run 16000
