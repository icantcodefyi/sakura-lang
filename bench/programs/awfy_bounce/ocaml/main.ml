(* Bounce from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/bounce.js and
   som.js's Random; Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks).
   Follows AWFY's typed ports: the balls are records with mutable fields in a fixed array and
   Random's seed is a mutable field. Random keeps OCaml's `land` (Sakura has no bitwise
   operators and takes `mod 65536`, the same value). run(n) sums n benchmark() results. *)

type random = { mutable seed : int }

let new_random () = { seed = 74755 }

let next random =
  random.seed <- ((random.seed * 1309) + 13849) land 65535;
  random.seed

type ball = { mutable x : int; mutable y : int; mutable x_vel : int; mutable y_vel : int }

let new_ball random =
  let x = next random mod 500 in
  let y = next random mod 500 in
  let x_vel = (next random mod 300) - 150 in
  let y_vel = (next random mod 300) - 150 in
  { x; y; x_vel; y_vel }

let bounce b =
  let x_limit = 500 in
  let y_limit = 500 in
  let bounced = ref false in

  b.x <- b.x + b.x_vel;
  b.y <- b.y + b.y_vel;

  if b.x > x_limit then begin
    b.x <- x_limit; b.x_vel <- 0 - abs b.x_vel; bounced := true
  end;

  if b.x < 0 then begin
    b.x <- 0; b.x_vel <- abs b.x_vel; bounced := true
  end;

  if b.y > y_limit then begin
    b.y <- y_limit; b.y_vel <- 0 - abs b.y_vel; bounced := true
  end;

  if b.y < 0 then begin
    b.y <- 0; b.y_vel <- abs b.y_vel; bounced := true
  end;

  !bounced

let benchmark () =
  let random = new_random () in
  let ball_count = 100 in
  let bounces = ref 0 in
  let balls = Array.init ball_count (fun _ -> new_ball random) in

  for _ = 0 to 49 do
    Array.iter (fun ball -> if bounce ball then incr bounces) balls
  done;
  !bounces

let run n =
  let sum = ref 0 in
  for _ = 1 to n do
    sum := !sum + benchmark ()
  done;
  !sum

let () = Runner.main run 10000
