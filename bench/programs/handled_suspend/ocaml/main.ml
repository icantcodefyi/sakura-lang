(* Deviation: no clock or scheduler; a second effect Pause, performed once at i = n/2, is
   handled by capturing the continuation, adding 1 virtual ms and resuming it (as Koka). *)
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

type _ Effect.t += Pause : unit Effect.t

let run n =
  let elapsed = ref 0 in
  try_with (fun () ->
    counter (fun () ->
      let acc = ref 0 in
      for i = 0 to n - 1 do
        acc := !acc + perform Next mod 7;
        if i = n / 2 then perform Pause
      done;
      !acc + !elapsed)) ()
    { effc = (fun (type a) (e : a Effect.t) ->
        match e with
        | Pause -> Some (fun (k : (a, _) continuation) -> incr elapsed; continue k ())
        | _ -> None) }

let () = Runner.main run 1000000
