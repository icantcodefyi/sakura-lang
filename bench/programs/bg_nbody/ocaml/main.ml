(* n-body, the Benchmarks Game's task: the Java program's symplectic integrator, printing the
   energy before and after n steps as `%.9f` lines. Deviation: bodies are records with mutable
   float fields updated in place (Sakura copies persistent records); the Float operation order is
   the Sakura program's, and each product is bound with `let` before it is added (ocamlopt on
   arm64 fuses `a +. b *. c` into fmadd/fmsub, which C and Go are kept from). `main output <n>`
   writes the program's output; run(n) is the digest of the same bytes (bench/README.md). *)
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
    let mx = b.vx *. b.mass and my = b.vy *. b.mass and mz = b.vz *. b.mass in
    px := !px +. mx;
    py := !py +. my;
    pz := !pz +. mz) bs;
  let sun = bs.(0) in
  sun.vx <- 0.0 -. !px /. solar_mass;
  sun.vy <- 0.0 -. !py /. solar_mass;
  sun.vz <- 0.0 -. !pz /. solar_mass

let energy bs =
  let n = Array.length bs in
  let e = ref 0.0 in
  for i = 0 to n - 1 do
    let b = bs.(i) in
    let vx2 = b.vx *. b.vx and vy2 = b.vy *. b.vy and vz2 = b.vz *. b.vz in
    let kinetic = 0.5 *. b.mass *. (vx2 +. vy2 +. vz2) in
    e := !e +. kinetic;
    for j = i + 1 to n - 1 do
      let c = bs.(j) in
      let dx = b.x -. c.x and dy = b.y -. c.y and dz = b.z -. c.z in
      let dx2 = dx *. dx and dy2 = dy *. dy and dz2 = dz *. dz in
      let potential = b.mass *. c.mass /. Float.sqrt (dx2 +. dy2 +. dz2) in
      e := !e -. potential
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
      let dx2 = dx *. dx and dy2 = dy *. dy and dz2 = dz *. dz in
      let d2 = dx2 +. dy2 +. dz2 in
      let mag = dt /. (d2 *. Float.sqrt d2) in
      let bx = dx *. c.mass *. mag and by = dy *. c.mass *. mag and bz = dz *. c.mass *. mag in
      b.vx <- b.vx -. bx;
      b.vy <- b.vy -. by;
      b.vz <- b.vz -. bz;
      let cx = dx *. b.mass *. mag and cy = dy *. b.mass *. mag and cz = dz *. b.mass *. mag in
      c.vx <- c.vx +. cx;
      c.vy <- c.vy +. cy;
      c.vz <- c.vz +. cz
    done
  done;
  Array.iter (fun b ->
    let sx = dt *. b.vx and sy = dt *. b.vy and sz = dt *. b.vz in
    b.x <- b.x +. sx;
    b.y <- b.y +. sy;
    b.z <- b.z +. sz) bs

(* What the Benchmarks Game program writes for n. *)
let text n =
  let bs = system () in
  offset_momentum bs;
  let out = Buffer.create 32 in
  Buffer.add_string out (Printf.sprintf "%.9f\n" (energy bs));
  for _ = 1 to n do advance bs 0.01 done;
  Buffer.add_string out (Printf.sprintf "%.9f\n" (energy bs));
  Buffer.contents out

(* h = (31·h + byte) mod 1,000,000,007 over every byte. *)
let digest s = String.fold_left (fun h c -> (h * 31 + Char.code c) mod 1000000007) 0 s

let maximum = 50000000

let () =
  if Array.length Sys.argv = 3 && Sys.argv.(1) = "output" then begin
    let n = int_of_string Sys.argv.(2) in
    if n < 0 || n > maximum then exit 2;
    print_string (text n)
  end else Runner.main (fun n -> digest (text n)) maximum
