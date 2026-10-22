(* Richards from Are We Fast Yet (benchmarks/JavaScript/richards.js), "a derivation from the SOM
   version, which is derived from Mario Wolczko's Smalltalk version"; see ../LICENSE.
   AWFY's objects are records with mutable fields linked by options (None is NO_TASK / NO_WORK),
   mutated in place. Deviation: AWFY's task functions are closures made with their data record;
   here the data record is a variant of mutable records and run_task matches on it, as every port
   of this program does. TRACING is false in AWFY; its trace code is left out. run(n) runs
   benchmark() n times and returns the number of true results. *)

let idler = 0
let worker = 1
let handler_a = 2
let handler_b = 3
let device_a = 4
let device_b = 5
let num_types = 6

let device_packet_kind = 0
let work_packet_kind = 1

let data_size = 4

type packet = {
  mutable link : packet option;
  mutable identity : int;
  kind : int;
  mutable datum : int;
  data : int array;
}

(* RBObject's append. *)
let append packet queue_head =
  packet.link <- None;
  match queue_head with
  | None -> Some packet
  | Some head ->
    let mouse = ref head in
    let rec walk () =
      match !mouse.link with
      | Some link -> mouse := link; walk ()
      | None -> ()
    in
    walk ();
    !mouse.link <- Some packet;
    queue_head

type device_data = { mutable pending : packet option }
type handler_data = { mutable work_in : packet option; mutable device_in : packet option }
type idle_data = { mutable control : int; mutable count : int }
type worker_data = { mutable destination : int; mutable wcount : int }

(* The task data records; the variant selects the task function. *)
type data =
  | Device of device_data
  | Handler of handler_data
  | Idle of idle_data
  | Worker of worker_data

type tcb = {
  link : tcb option;
  identity : int;
  priority : int;
  mutable input : packet option;
  mutable packet_pending : bool;
  mutable task_waiting : bool;
  mutable task_holding : bool;
  handle : data;
}

(* TaskState's constructors: packet_pending, task_waiting, task_holding. *)
let create_running () = (false, false, false)
let create_waiting () = (false, true, false)
let create_waiting_with_packet () = (true, true, false)

let is_task_holding_or_waiting t = t.task_holding || (not t.packet_pending && t.task_waiting)
let is_waiting_with_packet t = t.packet_pending && t.task_waiting && not t.task_holding

let add_input_and_check_priority t packet old_task =
  match t.input with
  | None ->
    t.input <- Some packet;
    t.packet_pending <- true;
    if t.priority > old_task.priority then t else old_task
  | Some _ ->
    t.input <- append packet t.input;
    old_task

type scheduler = {
  mutable queue_packet_count : int;
  mutable hold_count : int;
  task_table : tcb option array;
  mutable task_list : tcb option;
  mutable current_task : tcb option;
  mutable current_task_identity : int;
}

let current s = match s.current_task with Some t -> t | None -> invalid_arg "current task"

let find_task s identity =
  match s.task_table.(identity) with Some t -> t | None -> failwith "findTask failed"

let hold_self s =
  s.hold_count <- s.hold_count + 1;
  let t = current s in
  t.task_holding <- true;
  t.link

let queue_packet s (packet : packet) =
  let t = find_task s packet.identity in
  s.queue_packet_count <- s.queue_packet_count + 1;
  packet.link <- None;
  packet.identity <- s.current_task_identity;
  Some (add_input_and_check_priority t packet (current s))

let release s identity =
  let t = find_task s identity in
  t.task_holding <- false;
  if t.priority > (current s).priority then Some t else s.current_task

let mark_waiting s =
  (current s).task_waiting <- true;
  s.current_task

(* The task functions: AWFY's closures in createDevice, createHandler, createIdler and
   createWorker. *)

let device_fn s work_arg d =
  match work_arg with
  | None -> (
    match d.pending with
    | None -> mark_waiting s
    | Some function_work ->
      d.pending <- None;
      queue_packet s function_work)
  | Some function_work ->
    d.pending <- Some function_work;
    hold_self s

let handler_fn s (work : packet option) d =
  (match work with
   | Some w ->
     if w.kind = work_packet_kind then d.work_in <- append w d.work_in
     else d.device_in <- append w d.device_in
   | None -> ());
  match d.work_in with
  | None -> mark_waiting s
  | Some work_packet ->
    let count = work_packet.datum in
    if count >= data_size then begin
      d.work_in <- work_packet.link;
      queue_packet s work_packet
    end else
      match d.device_in with
      | None -> mark_waiting s
      | Some device_packet ->
        d.device_in <- device_packet.link;
        device_packet.datum <- work_packet.data.(count);
        work_packet.datum <- count + 1;
        queue_packet s device_packet

let idler_fn s d =
  d.count <- d.count - 1;
  if d.count = 0 then hold_self s
  else if d.control land 1 = 0 then begin
    d.control <- d.control / 2;
    release s device_a
  end else begin
    d.control <- (d.control / 2) lxor 53256;
    release s device_b
  end

let worker_fn s (work : packet option) d =
  match work with
  | None -> mark_waiting s
  | Some work ->
    d.destination <- (if d.destination = handler_a then handler_b else handler_a);
    work.identity <- d.destination;
    work.datum <- 0;
    for i = 0 to data_size - 1 do
      d.wcount <- d.wcount + 1;
      if d.wcount > 26 then d.wcount <- 1;
      work.data.(i) <- 65 + d.wcount - 1
    done;
    queue_packet s work

let run_task s t =
  let message =
    if is_waiting_with_packet t then begin
      let message = t.input in
      (match message with Some m -> t.input <- m.link | None -> ());
      (match t.input with
       | None -> (* running() *)
         t.packet_pending <- false; t.task_waiting <- false; t.task_holding <- false
       | Some _ -> (* packetPending() *)
         t.packet_pending <- true; t.task_waiting <- false; t.task_holding <- false);
      message
    end else None
  in
  match t.handle with
  | Device d -> device_fn s message d
  | Handler d -> handler_fn s message d
  | Idle d -> idler_fn s d
  | Worker d -> worker_fn s message d

let create_packet link identity kind =
  Some { link; identity; kind; datum = 0; data = Array.make data_size 0 }

let create_task s identity priority work (packet_pending, task_waiting, task_holding) handle =
  let t = { link = s.task_list; identity; priority; input = work;
            packet_pending; task_waiting; task_holding; handle } in
  s.task_list <- Some t;
  s.task_table.(identity) <- Some t

let schedule s =
  s.current_task <- s.task_list;
  let rec loop () =
    match s.current_task with
    | None -> ()
    | Some t ->
      if is_task_holding_or_waiting t then s.current_task <- t.link
      else begin
        s.current_task_identity <- t.identity;
        s.current_task <- run_task s t
      end;
      loop ()
  in
  loop ()

let start () =
  let s = { queue_packet_count = 0; hold_count = 0; task_table = Array.make num_types None;
            task_list = None; current_task = None; current_task_identity = 0 } in
  create_task s idler 0 None (create_running ()) (Idle { control = 1; count = 10000 });
  let work_q = create_packet None worker work_packet_kind in
  let work_q = create_packet work_q worker work_packet_kind in

  create_task s worker 1000 work_q (create_waiting_with_packet ())
    (Worker { destination = handler_a; wcount = 0 });
  let work_q = create_packet None device_a device_packet_kind in
  let work_q = create_packet work_q device_a device_packet_kind in
  let work_q = create_packet work_q device_a device_packet_kind in

  create_task s handler_a 2000 work_q (create_waiting_with_packet ())
    (Handler { work_in = None; device_in = None });
  let work_q = create_packet None device_b device_packet_kind in
  let work_q = create_packet work_q device_b device_packet_kind in
  let work_q = create_packet work_q device_b device_packet_kind in

  create_task s handler_b 3000 work_q (create_waiting_with_packet ())
    (Handler { work_in = None; device_in = None });
  create_task s device_a 4000 None (create_waiting ()) (Device { pending = None });
  create_task s device_b 5000 None (create_waiting ()) (Device { pending = None });

  schedule s;

  s.queue_packet_count = 23246 && s.hold_count = 9297

let run inner_iterations =
  let sum = ref 0 in
  for _ = 1 to inner_iterations do
    if start () then incr sum
  done;
  !sum

let () = Runner.main run 100000
