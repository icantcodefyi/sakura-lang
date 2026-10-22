(* No deviation: Next is performed from the callback List.fold_left calls over 0..n-1. *)
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

let run n =
  counter (fun () -> List.fold_left (fun acc _ -> acc + perform Next mod 7) 0 (List.init n Fun.id))

let () = Runner.main run 1000000
