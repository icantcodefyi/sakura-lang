(* NBody from Are We Fast Yet (benchmarks/JavaScript/nbody.js: the Benchmarks Game's n-body,
   contributed by Mark C. Lewis, modified slightly by Chad Whipkey; revised BSD licence). AWFY's
   Body as a record with mutable fields, NBodySystem as functions over an array of them, updated
   in place in AWFY's operation order. Every product that is added or subtracted is bound by `let`
   first: ocamlopt on arm64 otherwise fuses `a +. b *. c` into one fmadd (no flag turns it off),
   which changes the last bits. run(n) is the energy after n advances times 10^16, rounded half
   away from zero (Float.round). *)

let pi = 3.141592653589793
let solar_mass = 4.0 *. pi *. pi
let days_per_yer = 365.24

type body = {
  mutable x : float;
  mutable y : float;
  mutable z : float;
  mutable vx : float;
  mutable vy : float;
  mutable vz : float;
  mass : float;
}

let body x y z vx vy vz mass =
  { x; y; z; vx = vx *. days_per_yer; vy = vy *. days_per_yer; vz = vz *. days_per_yer;
    mass = mass *. solar_mass }

let offset_momentum b px py pz =
  b.vx <- 0.0 -. (px /. solar_mass);
  b.vy <- 0.0 -. (py /. solar_mass);
  b.vz <- 0.0 -. (pz /. solar_mass)

let jupiter () =
  body 4.84143144246472090e+00 (-1.16032004402742839e+00) (-1.03622044471123109e-01)
    1.66007664274403694e-03 7.69901118419740425e-03 (-6.90460016972063023e-05)
    9.54791938424326609e-04

let saturn () =
  body 8.34336671824457987e+00 4.12479856412430479e+00 (-4.03523417114321381e-01)
    (-2.76742510726862411e-03) 4.99852801234917238e-03 2.30417297573763929e-05
    2.85885980666130812e-04

let uranus () =
  body 1.28943695621391310e+01 (-1.51111514016986312e+01) (-2.23307578892655734e-01)
    2.96460137564761618e-03 2.37847173959480950e-03 (-2.96589568540237556e-05)
    4.36624404335156298e-05

let neptune () =
  body 1.53796971148509165e+01 (-2.59193146099879641e+01) 1.79258772950371181e-01
    2.68067772490389322e-03 1.62824170038242295e-03 (-9.51592254519715870e-05)
    5.15138902046611451e-05

let sun () = body 0.0 0.0 0.0 0.0 0.0 0.0 1.0

let create_bodies () =
  let bodies = [| sun (); jupiter (); saturn (); uranus (); neptune () |] in
  let px = ref 0.0 and py = ref 0.0 and pz = ref 0.0 in
  Array.iter (fun b ->
    let mx = b.vx *. b.mass and my = b.vy *. b.mass and mz = b.vz *. b.mass in
    px := !px +. mx;
    py := !py +. my;
    pz := !pz +. mz) bodies;
  offset_momentum bodies.(0) !px !py !pz;
  bodies

let advance bodies dt =
  let n = Array.length bodies in
  for i = 0 to n - 1 do
    let i_body = bodies.(i) in
    for j = i + 1 to n - 1 do
      let j_body = bodies.(j) in
      let dx = i_body.x -. j_body.x in
      let dy = i_body.y -. j_body.y in
      let dz = i_body.z -. j_body.z in
      let dx2 = dx *. dx and dy2 = dy *. dy and dz2 = dz *. dz in
      let d_squared = dx2 +. dy2 +. dz2 in
      let distance = sqrt d_squared in
      let mag = dt /. (d_squared *. distance) in
      let ix = dx *. j_body.mass *. mag and iy = dy *. j_body.mass *. mag
      and iz = dz *. j_body.mass *. mag in
      i_body.vx <- i_body.vx -. ix;
      i_body.vy <- i_body.vy -. iy;
      i_body.vz <- i_body.vz -. iz;
      let jx = dx *. i_body.mass *. mag and jy = dy *. i_body.mass *. mag
      and jz = dz *. i_body.mass *. mag in
      j_body.vx <- j_body.vx +. jx;
      j_body.vy <- j_body.vy +. jy;
      j_body.vz <- j_body.vz +. jz
    done
  done;
  Array.iter (fun b ->
    let sx = dt *. b.vx and sy = dt *. b.vy and sz = dt *. b.vz in
    b.x <- b.x +. sx;
    b.y <- b.y +. sy;
    b.z <- b.z +. sz) bodies

let energy bodies =
  let n = Array.length bodies in
  let e = ref 0.0 in
  for i = 0 to n - 1 do
    let i_body = bodies.(i) in
    let vx2 = i_body.vx *. i_body.vx and vy2 = i_body.vy *. i_body.vy
    and vz2 = i_body.vz *. i_body.vz in
    let kinetic = 0.5 *. i_body.mass *. (vx2 +. vy2 +. vz2) in
    e := !e +. kinetic;
    for j = i + 1 to n - 1 do
      let j_body = bodies.(j) in
      let dx = i_body.x -. j_body.x in
      let dy = i_body.y -. j_body.y in
      let dz = i_body.z -. j_body.z in
      let dx2 = dx *. dx and dy2 = dy *. dy and dz2 = dz *. dz in
      let distance = sqrt (dx2 +. dy2 +. dz2) in
      e := !e -. (i_body.mass *. j_body.mass) /. distance
    done
  done;
  !e

let run inner_iterations =
  let system = create_bodies () in
  for _ = 1 to inner_iterations do advance system 0.01 done;
  int_of_float (Float.round (energy system *. 1.0e16))

let () = Runner.main run 1000000
