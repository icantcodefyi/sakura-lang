(* Deviation: the 15 padding handlers share one Pad effect indexed by k (Sakura and Koka
   declare one effect each); none is performed, so Next passes all 15 frames. *)
open Effect
open Effect.Deep

type _ Effect.t += Next : int Effect.t

let counter body =
  let value = ref 0 in
  try_with body ()
    { effc = (fun (type a) (e : a Effect.t) ->
        match e with
        | Next -> Some (fun (k : (a, _) continuation) -> incr value; continue k !value)
        | _ -> None) }

let tick_loop n =
  let acc = ref 0 in
  for _ = 1 to n do acc := !acc + perform Next mod 7 done;
  !acc

type _ Effect.t += Pad : int -> int Effect.t

(* An unrelated handler frame: it answers only Pad k and is never performed. *)
let pad k body =
  try_with body ()
    { effc = (fun (type a) (e : a Effect.t) ->
        match e with
        | Pad k' when k' = k -> Some (fun (c : (a, _) continuation) -> continue c k)
        | _ -> None) }

let run n =
  let rec padded k = if k > 15 then tick_loop n else pad k (fun () -> padded (k + 1)) in
  counter (fun () -> padded 1)

let () = Runner.main run 1000000
