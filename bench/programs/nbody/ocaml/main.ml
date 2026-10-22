(* Deviation: bodies are records with mutable float fields updated in place (Sakura copies
   persistent records); the Float operation order is the source's. *)
type body = {
  mutable x : float; mutable y : float; mutable z : float;
  mutable vx : float; mutable vy : float; mutable vz : float;
  mass : float;
}

let pi = 3.141592653589793
let solar_mass = 4.0 *. pi *. pi
let days = 365.24

let planet x y z vx vy vz m =
  { x; y; z; vx = vx *. days; vy = vy *. days; vz = vz *. days; mass = m *. solar_mass }

let system () = [|
  { x = 0.0; y = 0.0; z = 0.0; vx = 0.0; vy = 0.0; vz = 0.0; mass = solar_mass };
  planet 4.84143144246472090e+00 (-1.16032004402742839e+00) (-1.03622044471123109e-01)
    1.66007664274403694e-03 7.69901118419740425e-03 (-6.90460016972063023e-05)
    9.54791938424326609e-04;
  planet 8.34336671824457987e+00 4.12479856412430479e+00 (-4.03523417114321381e-01)
    (-2.76742510726862411e-03) 4.99852801234917238e-03 2.30417297573763929e-05
    2.85885980666130812e-04;
  planet 1.28943695621391310e+01 (-1.51111514016986312e+01) (-2.23307578892655734e-01)
    2.96460137564761618e-03 2.37847173959480950e-03 (-2.96589568540237556e-05)
    4.36624404335156298e-05;
  planet 1.53796971148509165e+01 (-2.59193146099879641e+01) 1.79258772950371181e-01
    2.68067772490389322e-03 1.62824170038242295e-03 (-9.51592254519715870e-05)
    5.15138902046611451e-05;
|]

let offset_momentum bs =
  let px = ref 0.0 and py = ref 0.0 and pz = ref 0.0 in
  Array.iter (fun b ->
    px := !px +. b.vx *. b.mass;
    py := !py +. b.vy *. b.mass;
    pz := !pz +. b.vz *. b.mass) bs;
  let sun = bs.(0) in
  sun.vx <- 0.0 -. !px /. solar_mass;
  sun.vy <- 0.0 -. !py /. solar_mass;
  sun.vz <- 0.0 -. !pz /. solar_mass

let energy bs =
  let n = Array.length bs in
  let e = ref 0.0 in
  for i = 0 to n - 1 do
    let b = bs.(i) in
    e := !e +. 0.5 *. b.mass *. (b.vx *. b.vx +. b.vy *. b.vy +. b.vz *. b.vz);
    for j = i + 1 to n - 1 do
      let c = bs.(j) in
      let dx = b.x -. c.x and dy = b.y -. c.y and dz = b.z -. c.z in
      e := !e -. b.mass *. c.mass /. Float.sqrt (dx *. dx +. dy *. dy +. dz *. dz)
    done
  done;
  !e

let advance bs dt =
  let n = Array.length bs in
  for i = 0 to n - 1 do
    let b = bs.(i) in
    for j = i + 1 to n - 1 do
      let c = bs.(j) in
      let dx = b.x -. c.x and dy = b.y -. c.y and dz = b.z -. c.z in
      let d2 = dx *. dx +. dy *. dy +. dz *. dz in
      let mag = dt /. (d2 *. Float.sqrt d2) in
      b.vx <- b.vx -. dx *. c.mass *. mag;
      b.vy <- b.vy -. dy *. c.mass *. mag;
      b.vz <- b.vz -. dz *. c.mass *. mag;
      c.vx <- c.vx +. dx *. b.mass *. mag;
      c.vy <- c.vy +. dy *. b.mass *. mag;
      c.vz <- c.vz +. dz *. b.mass *. mag
    done
  done;
  Array.iter (fun b ->
    b.x <- b.x +. dt *. b.vx;
    b.y <- b.y +. dt *. b.vy;
    b.z <- b.z +. dt *. b.vz) bs

let run n =
  let bs = system () in
  offset_momentum bs;
  for _ = 1 to n do advance bs 0.01 done;
  int_of_float (Float.round (energy bs *. 1.0e9))

let () = Runner.main run 100000
