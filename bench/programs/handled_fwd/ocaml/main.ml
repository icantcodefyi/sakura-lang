(* No deviation: a wrapper handler re-performs Next to the outer counter (as Koka). *)
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

let forward body =
  try_with body ()
    { effc = (fun (type a) (e : a Effect.t) ->
        match e with
        | Next -> Some (fun (k : (a, _) continuation) -> continue k (perform Next))
        | _ -> None) }

let run n = counter (fun () -> forward (fun () -> tick_loop n))

let () = Runner.main run 1000000
