(* Mandelbrot from Are We Fast Yet (benchmarks/JavaScript/mandelbrot.js; revised BSD licence,
   Copyright © 2004-2013 Brent Fulgham; Benchmarks Game program contributed by Karl von
   Laudermann, modified by Jeremy Echols, Detlef Reichl, Joseph LaFata and Peter Zotov).
   Same loop as the JS; the shifts and XOR are OCaml's operators (Sakura multiplies and loops). *)

let mandelbrot size =
  let sum = ref 0 in
  let byte_acc = ref 0 in
  let bit_num = ref 0 in
  let y = ref 0 in
  while !y < size do
    let ci = ((2.0 *. float_of_int !y) /. float_of_int size) -. 1.0 in
    let x = ref 0 in
    while !x < size do
      let zrzr = ref 0.0 in
      let zi = ref 0.0 in
      let zizi = ref 0.0 in
      let cr = ((2.0 *. float_of_int !x) /. float_of_int size) -. 1.5 in
      let z = ref 0 in
      let not_done = ref true in
      let escape = ref 0 in
      while !not_done && !z < 50 do
        let zr = !zrzr -. !zizi +. cr in
        let t = 2.0 *. zr *. !zi in (* bound first: ocamlopt on arm64 may fuse a +. b *. c *)
        zi := t +. ci;
        (* preserve recalculation *)
        zrzr := zr *. zr;
        zizi := !zi *. !zi;
        if !zrzr +. !zizi > 4.0 then begin
          not_done := false;
          escape := 1
        end;
        incr z
      done;
      byte_acc := (!byte_acc lsl 1) + !escape;
      incr bit_num;
      if !bit_num = 8 then begin
        sum := !sum lxor !byte_acc;
        byte_acc := 0;
        bit_num := 0
      end else if !x = size - 1 then begin
        byte_acc := !byte_acc lsl (8 - !bit_num);
        sum := !sum lxor !byte_acc;
        byte_acc := 0;
        bit_num := 0
      end;
      incr x
    done;
    incr y
  done;
  !sum

let () = Runner.main mandelbrot 1000
