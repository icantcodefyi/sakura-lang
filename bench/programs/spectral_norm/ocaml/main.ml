(* Deviation: float arrays built with Array.init (Sakura pushes onto persistent Lists);
   the Float operation order is the source's. *)
let a i j = 1.0 /. float_of_int (((i + j) * (i + j + 1) / 2) + i + 1)

let mul_av v n =
  Array.init n (fun i ->
    let sum = ref 0.0 in
    for j = 0 to n - 1 do sum := !sum +. a i j *. v.(j) done;
    !sum)

let mul_atv v n =
  Array.init n (fun i ->
    let sum = ref 0.0 in
    for j = 0 to n - 1 do sum := !sum +. a j i *. v.(j) done;
    !sum)

let mul_atav v n = mul_atv (mul_av v n) n

let run n =
  if n = 0 then 0
  else begin
    let u = ref (Array.make n 1.0) in
    let v = ref !u in
    for _ = 1 to 10 do
      v := mul_atav !u n;
      u := mul_atav !v n
    done;
    let vbv = ref 0.0 and vv = ref 0.0 in
    for m = 0 to n - 1 do
      vbv := !vbv +. !u.(m) *. !v.(m);
      vv := !vv +. !v.(m) *. !v.(m)
    done;
    int_of_float (Float.round (Float.sqrt (!vbv /. !vv) *. 1.0e9))
  end

let () = Runner.main run 400
