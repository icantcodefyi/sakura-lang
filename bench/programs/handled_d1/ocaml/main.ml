(* No deviation: an OCaml 5 deep handler for Next, one frame from the perform (as Koka). *)
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

let run n = counter (fun () -> tick_loop n)

let () = Runner.main run 1000000
