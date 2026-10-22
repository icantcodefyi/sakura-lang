(* Towers from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/towers.js;
   Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's
   typed ports: disks are records linked through a mutable `next` option, three piles are an
   array of tops, and every move relinks a disk in place. An illegal move raises Failure with
   AWFY's message. run(n) sums n benchmark() results. *)

type disk = { size : int; mutable next : disk option }

type towers = { piles : disk option array; mutable moves_done : int }

let push_disk t disk pile =
  let top = t.piles.(pile) in
  (match top with
   | Some top when disk.size >= top.size -> failwith "Cannot put a big disk on a smaller one"
   | _ -> ());
  disk.next <- top;
  t.piles.(pile) <- Some disk

let pop_disk_from t pile =
  match t.piles.(pile) with
  | None -> failwith "Attempting to remove a disk from an empty pile"
  | Some top ->
      t.piles.(pile) <- top.next;
      top.next <- None;
      top

let move_top_disk t from_pile to_pile =
  push_disk t (pop_disk_from t from_pile) to_pile;
  t.moves_done <- t.moves_done + 1

let build_tower_at t pile disks =
  for i = disks downto 0 do
    push_disk t { size = i; next = None } pile
  done

let rec move_disks t disks from_pile to_pile =
  if disks = 1 then move_top_disk t from_pile to_pile
  else begin
    let other_pile = 3 - from_pile - to_pile in
    move_disks t (disks - 1) from_pile other_pile;
    move_top_disk t from_pile to_pile;
    move_disks t (disks - 1) other_pile to_pile
  end

let benchmark t =
  Array.fill t.piles 0 3 None;
  build_tower_at t 0 13;
  t.moves_done <- 0;
  move_disks t 13 0 1;
  t.moves_done

let run n =
  let bench = { piles = Array.make 3 None; moves_done = 0 } in
  let sum = ref 0 in
  for _ = 1 to n do
    sum := !sum + benchmark bench
  done;
  !sum

let () = Runner.main run 2500
