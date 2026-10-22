(* Havlak loop recognition from Are We Fast Yet (smarr/are-we-fast-yet@74306fec
   benchmarks/JavaScript/havlak.js and Java/src/havlak; Apache-2.0, Copyright 2011 Google Inc.;
   the som collections MIT, Copyright (c) 2015-2016 Stefan Marr; see ../LICENSE). run n is
   LoopTesterApp.main(n, 50, 10, 10, 5) as loops * 1000000 + nodes.
   Objects are records with mutable fields linked directly; identity is physical equality (==).
   som's Vector is a doubling array with a first index; `at`/`at_put` (used by the block map
   only) work on a vector of options, as OCaml has no null. Set and IdentitySet are one type
   (== on ints is equality). IdentityDictionary is som's chained hash table keyed by the block
   with the block's customHash (its name). forEach and hasSome take closures, as in AWFY. *)

let initial_size = 10
let initial_capacity = 16

(* ---- som: Vector *)

module Vector = struct
  type 'a t = { mutable storage : 'a array; mutable first : int; mutable last : int }

  let create () = { storage = [||]; first = 0; last = 0 }

  let get v idx = v.storage.(idx)

  (* som's `at` on the block map: None past the end. *)
  let at v idx = if idx >= Array.length v.storage then None else v.storage.(idx)

  let at_put v idx x =
    if Array.length v.storage = 0 then v.storage <- Array.make (max (idx + 1) initial_size) None
    else if idx >= Array.length v.storage then begin
      let new_length = ref (Array.length v.storage) in
      while !new_length <= idx do new_length := !new_length * 2 done;
      let storage = Array.make !new_length None in
      Array.blit v.storage 0 storage 0 (Array.length v.storage);
      v.storage <- storage
    end;
    v.storage.(idx) <- Some x;
    if v.last < idx + 1 then v.last <- idx + 1

  let append v elem =
    if Array.length v.storage = 0 then v.storage <- Array.make initial_size elem
    else if v.last >= Array.length v.storage then begin
      let storage = Array.make (Array.length v.storage * 2) elem in
      Array.blit v.storage 0 storage 0 v.last;
      v.storage <- storage
    end;
    v.storage.(v.last) <- elem;
    v.last <- v.last + 1

  let is_empty v = v.last = v.first
  let size v = v.last - v.first

  let iter f v = for i = v.first to v.last - 1 do f v.storage.(i) done

  let has_some f v =
    let rec go i = i < v.last && (f v.storage.(i) || go (i + 1)) in
    go v.first

  (* som's `removeFirst` on a non-empty vector. *)
  let remove_first v =
    v.first <- v.first + 1;
    v.storage.(v.first - 1)

  let remove_all v =
    v.first <- 0;
    v.last <- 0;
    v.storage <- [||]
end

(* ---- som: Set and IdentitySet *)

module Set = struct
  type 'a t = { items : 'a Vector.t }

  let create () = { items = Vector.create () }
  let size s = Vector.size s.items
  let iter f s = Vector.iter f s.items
  let contains s obj = Vector.has_some (fun e -> e == obj) s.items
  let add s obj = if not (contains s obj) then Vector.append s.items obj
end

(* ---- BasicBlock, BasicBlockEdge, ControlFlowGraph *)

type basic_block = {
  in_edges : basic_block Vector.t;
  out_edges : basic_block Vector.t;
  name : int;
}

let basic_block name = { in_edges = Vector.create (); out_edges = Vector.create (); name }
let num_pred bb = Vector.size bb.in_edges
let custom_hash bb = bb.name

type basic_block_edge = { from : basic_block; to_ : basic_block }

type control_flow_graph = {
  basic_block_map : basic_block option Vector.t;
  mutable start_node : basic_block option;
  edge_list : basic_block_edge Vector.t;
}

let control_flow_graph () =
  { basic_block_map = Vector.create (); start_node = None; edge_list = Vector.create () }

let num_nodes cfg = Vector.size cfg.basic_block_map

let create_node cfg name =
  let node =
    match Vector.at cfg.basic_block_map name with
    | Some node -> node
    | None ->
      let node = basic_block name in
      Vector.at_put cfg.basic_block_map name node;
      node
  in
  if num_nodes cfg = 1 then cfg.start_node <- Some node;
  node

(* `new BasicBlockEdge(cfg, fromName, toName)`. *)
let basic_block_edge cfg from_name to_name =
  let from = create_node cfg from_name in
  let to_ = create_node cfg to_name in
  let edge = { from; to_ } in
  Vector.append edge.from.out_edges edge.to_;
  Vector.append edge.to_.in_edges edge.from;
  Vector.append cfg.edge_list edge

(* ---- som: IdentityDictionary from a basic block to an int *)

type dict_entry = {
  hash : int;
  key : basic_block;
  mutable value : int;
  mutable next : dict_entry option;
}

type identity_dictionary = { mutable buckets : dict_entry option array; mutable size : int }

let hash_fn key =
  let hash = custom_hash key in
  hash lxor (hash lsr 16)

let identity_dictionary () = { buckets = Array.make initial_capacity None; size = 0 }
let bucket_idx d hash = (Array.length d.buckets - 1) land hash
let matches e hash key = e.hash = hash && e.key == key

let dict_at d key =
  let hash = hash_fn key in
  let rec go = function
    | None -> None
    | Some e -> if matches e hash key then Some e.value else go e.next
  in
  go d.buckets.(bucket_idx d hash)

let new_entry key value hash = Some { hash; key; value; next = None }

let insert_bucket_entry d key value hash head =
  let rec go current =
    if matches current hash key then current.value <- value
    else
      match current.next with
      | None ->
        d.size <- d.size + 1;
        current.next <- new_entry key value hash
      | Some next -> go next
  in
  go head

let split_bucket d old_storage i head =
  let old_length = Array.length old_storage in
  let lo_head = ref None and lo_tail = ref None and hi_head = ref None and hi_tail = ref None in
  let current = ref (Some head) in
  while Option.is_some !current do
    let c = Option.get !current in
    if c.hash land old_length = 0 then begin
      (match !lo_tail with None -> lo_head := !current | Some t -> t.next <- !current);
      lo_tail := !current
    end else begin
      (match !hi_tail with None -> hi_head := !current | Some t -> t.next <- !current);
      hi_tail := !current
    end;
    current := c.next
  done;
  (match !lo_tail with
   | Some t ->
     t.next <- None;
     d.buckets.(i) <- !lo_head
   | None -> ());
  match !hi_tail with
  | Some t ->
    t.next <- None;
    d.buckets.(i + old_length) <- !hi_head
  | None -> ()

let transfer_entries d old_storage =
  for i = 0 to Array.length old_storage - 1 do
    match old_storage.(i) with
    | None -> ()
    | Some current ->
      old_storage.(i) <- None;
      if Option.is_none current.next then
        d.buckets.(current.hash land (Array.length d.buckets - 1)) <- Some current
      else split_bucket d old_storage i current
  done

let resize d =
  let old_storage = d.buckets in
  d.buckets <- Array.make (Array.length old_storage * 2) None;
  transfer_entries d old_storage

let dict_at_put d key value =
  let hash = hash_fn key in
  let i = bucket_idx d hash in
  (match d.buckets.(i) with
   | None ->
     d.buckets.(i) <- new_entry key value hash;
     d.size <- d.size + 1
   | Some current -> insert_bucket_entry d key value hash current);
  if d.size > Array.length d.buckets then resize d

let dict_remove_all d =
  d.buckets <- Array.make (Array.length d.buckets) None;
  d.size <- 0

(* ---- SimpleLoop, LoopStructureGraph *)

type simple_loop = {
  basic_blocks : basic_block Set.t;
  children : simple_loop Set.t;
  mutable parent : simple_loop option;
  header : basic_block option;
  is_reducible : bool;
  mutable is_root : bool;
  mutable nesting_level : int;
  mutable counter : int;
  mutable depth_level : int;
}

let simple_loop bb is_reducible =
  let loop = {
    basic_blocks = Set.create ();
    children = Set.create ();
    parent = None;
    header = bb;
    is_reducible;
    is_root = false;
    nesting_level = 0;
    counter = 0;
    depth_level = 0;
  } in
  Option.iter (Set.add loop.basic_blocks) bb;
  loop

let set_parent loop parent =
  loop.parent <- Some parent;
  Set.add parent.children loop

let set_nesting_level loop level =
  loop.nesting_level <- level;
  if level = 0 then loop.is_root <- true

type loop_structure_graph = {
  root : simple_loop;
  loops : simple_loop Vector.t;
  mutable loop_counter : int;
}

let loop_structure_graph () =
  let lsg = { root = simple_loop None true; loops = Vector.create (); loop_counter = 0 } in
  set_nesting_level lsg.root 0;
  lsg.root.counter <- lsg.loop_counter;
  lsg.loop_counter <- lsg.loop_counter + 1;
  Vector.append lsg.loops lsg.root;
  lsg

let create_new_loop lsg bb is_reducible =
  let loop = simple_loop (Some bb) is_reducible in
  loop.counter <- lsg.loop_counter;
  lsg.loop_counter <- lsg.loop_counter + 1;
  Vector.append lsg.loops loop;
  loop

let rec calculate_nesting_level_rec loop depth =
  loop.depth_level <- depth;
  Set.iter
    (fun liter ->
      calculate_nesting_level_rec liter (depth + 1);
      set_nesting_level loop (max loop.nesting_level (1 + liter.nesting_level)))
    loop.children

let calculate_nesting_level lsg =
  (* link up all 1st level loops to artificial root node. *)
  Vector.iter
    (fun liter -> if (not liter.is_root) && Option.is_none liter.parent then set_parent liter lsg.root)
    lsg.loops;
  (* recursively traverse the tree and assign levels. *)
  calculate_nesting_level_rec lsg.root 0

let num_loops lsg = Vector.size lsg.loops

(* ---- UnionFindNode *)

type union_find_node = {
  mutable parent : union_find_node;
  mutable bb : basic_block option;
  mutable loop : simple_loop option;
  mutable dfs_number : int;
}

let union_find_node () =
  let rec node = { parent = node; bb = None; loop = None; dfs_number = 0 } in
  node

let init_node node bb dfs_number =
  node.parent <- node;
  node.bb <- Some bb;
  node.dfs_number <- dfs_number;
  node.loop <- None

let union node basic_block = node.parent <- basic_block

(* The find routine, with path compression: the nodes on the path whose parent is not a root
   are pointed at this node's parent. *)
let find_set self =
  let node_list = Vector.create () in
  let node = ref self in
  while !node != !node.parent do
    if !node.parent != !node.parent.parent then Vector.append node_list !node;
    node := !node.parent
  done;
  Vector.iter (fun iter -> union iter self.parent) node_list;
  !node

(* ---- HavlakLoopFinder *)

type bb_class = Bb_top | Bb_nonheader | Bb_reducible | Bb_self | Bb_irreducible | Bb_dead

let unvisited = 2147483647 (* Marker for uninitialized nodes. *)
let max_non_back_preds = 32 * 1024 (* Safeguard against pathological algorithm behavior. *)

type havlak_loop_finder = {
  cfg : control_flow_graph;
  lsg : loop_structure_graph;
  non_back_preds : int Set.t Vector.t;
  back_preds : int Vector.t Vector.t;
  number : identity_dictionary;
  mutable max_size : int;
  mutable header : int array;
  mutable typ : bb_class array;
  mutable last : int array;
  mutable nodes : union_find_node array;
}

let havlak_loop_finder cfg lsg = {
  cfg;
  lsg;
  non_back_preds = Vector.create ();
  back_preds = Vector.create ();
  number = identity_dictionary ();
  max_size = 0;
  header = [||];
  typ = [||];
  last = [||];
  nodes = [||];
}

let is_ancestor f w v = w <= v && v <= f.last.(w)

let rec do_dfs f current_node current =
  init_node f.nodes.(current) current_node current;
  dict_at_put f.number current_node current;
  let last_id = ref current in
  let outer_blocks = current_node.out_edges in
  for i = 0 to Vector.size outer_blocks - 1 do
    let target = Vector.get outer_blocks i in
    if Option.get (dict_at f.number target) = unvisited then last_id := do_dfs f target (!last_id + 1)
  done;
  f.last.(current) <- !last_id;
  !last_id

let init_all_nodes f =
  Vector.iter (fun bb -> dict_at_put f.number (Option.get bb) unvisited) f.cfg.basic_block_map;
  ignore (do_dfs f (Option.get f.cfg.start_node) 0)

let process_edges f node_w w =
  if num_pred node_w > 0 then
    Vector.iter
      (fun node_v ->
        let v = Option.get (dict_at f.number node_v) in
        if v <> unvisited then
          if is_ancestor f w v then Vector.append (Vector.get f.back_preds w) v
          else Set.add (Vector.get f.non_back_preds w) v)
      node_w.in_edges

let identify_edges f size =
  for w = 0 to size - 1 do
    f.header.(w) <- 0;
    f.typ.(w) <- Bb_nonheader;
    match f.nodes.(w).bb with
    | None -> f.typ.(w) <- Bb_dead
    | Some node_w -> process_edges f node_w w
  done

let step_d f w node_pool =
  Vector.iter
    (fun v -> if v <> w then Vector.append node_pool (find_set f.nodes.(v)) else f.typ.(w) <- Bb_self)
    (Vector.get f.back_preds w)

let step_e_process_non_back_preds f w node_pool work_list x =
  Set.iter
    (fun iter ->
      let y = f.nodes.(iter) in
      let ydash = find_set y in
      if not (is_ancestor f w ydash.dfs_number) then begin
        f.typ.(w) <- Bb_irreducible;
        Set.add (Vector.get f.non_back_preds w) ydash.dfs_number
      end
      else if ydash.dfs_number <> w then
        if not (Vector.has_some (fun e -> e == ydash) node_pool) then begin
          Vector.append work_list ydash;
          Vector.append node_pool ydash
        end)
    (Vector.get f.non_back_preds x.dfs_number)

let set_loop_attributes f w node_pool loop =
  f.nodes.(w).loop <- Some loop;
  Vector.iter
    (fun node ->
      (* Add nodes to loop descriptor. *)
      f.header.(node.dfs_number) <- w;
      union node f.nodes.(w);
      (* Nested loops are not added, but linked together. *)
      match node.loop with
      | Some l -> set_parent l loop
      | None -> Set.add loop.basic_blocks (Option.get node.bb))
    node_pool

exception Degenerate

let find_loops f =
  match f.cfg.start_node with
  | None -> ()
  | Some _ -> (
    let size = num_nodes f.cfg in
    Vector.remove_all f.non_back_preds;
    Vector.remove_all f.back_preds;
    dict_remove_all f.number;
    if size > f.max_size then begin
      f.header <- Array.make size 0;
      f.typ <- Array.make size Bb_top;
      f.last <- Array.make size 0;
      f.nodes <- Array.make size (union_find_node ());
      f.max_size <- size
    end;
    for i = 0 to size - 1 do
      Vector.append f.non_back_preds (Set.create ());
      Vector.append f.back_preds (Vector.create ());
      f.nodes.(i) <- union_find_node ()
    done;
    init_all_nodes f;
    identify_edges f size;
    (* Start node is root of all other loops. *)
    f.header.(0) <- 0;
    (* Step c: reverse DFS preorder, so inner loop headers come before the loops around them. *)
    try
      for w = size - 1 downto 0 do
        let node_pool = Vector.create () (* 'P' in Havlak's paper *) in
        match f.nodes.(w).bb with
        | None -> ()
        | Some node_w ->
          step_d f w node_pool;
          (* Copy nodePool to workList. *)
          let work_list = Vector.create () in
          Vector.iter (fun niter -> Vector.append work_list niter) node_pool;
          if Vector.size node_pool <> 0 then f.typ.(w) <- Bb_reducible;
          (* work the list... *)
          while not (Vector.is_empty work_list) do
            let x = Vector.remove_first work_list in
            (* Step e: the algorithm has degenerated past the safeguard: give up. *)
            let non_back_size = Set.size (Vector.get f.non_back_preds x.dfs_number) in
            if non_back_size > max_non_back_preds then raise Degenerate;
            step_e_process_non_back_preds f w node_pool work_list x
          done;
          (* Collapse the nodes of each SCC found into one, with a loop descriptor linked in. *)
          if Vector.size node_pool > 0 || f.typ.(w) = Bb_self then begin
            let loop = create_new_loop f.lsg node_w (f.typ.(w) <> Bb_irreducible) in
            set_loop_attributes f w node_pool loop
          end
      done
    with Degenerate -> ())

(* ---- LoopTesterApp *)

type loop_tester_app = { cfg : control_flow_graph; lsg : loop_structure_graph }

let loop_tester_app () =
  let app = { cfg = control_flow_graph (); lsg = loop_structure_graph () } in
  ignore (create_node app.cfg 0);
  app

let build_diamond app start =
  let bb0 = start in
  basic_block_edge app.cfg bb0 (bb0 + 1);
  basic_block_edge app.cfg bb0 (bb0 + 2);
  basic_block_edge app.cfg (bb0 + 1) (bb0 + 3);
  basic_block_edge app.cfg (bb0 + 2) (bb0 + 3);
  bb0 + 3

let build_connect app start end_ = basic_block_edge app.cfg start end_

let build_straight app start n =
  for i = 0 to n - 1 do build_connect app (start + i) (start + i + 1) done;
  start + n

let build_base_loop app from =
  let header = build_straight app from 1 in
  let diamond1 = build_diamond app header in
  let d11 = build_straight app diamond1 1 in
  let diamond2 = build_diamond app d11 in
  let footer = build_straight app diamond2 1 in
  build_connect app diamond2 d11;
  build_connect app diamond1 header;
  build_connect app footer from;
  build_straight app footer 1

let app_find_loops (app : loop_tester_app) loop_structure =
  find_loops (havlak_loop_finder app.cfg loop_structure)

let construct_simple_cfg app =
  ignore (create_node app.cfg 0);
  ignore (build_base_loop app 0);
  ignore (create_node app.cfg 1);
  basic_block_edge app.cfg 0 2

let add_dummy_loops app num_dummy_loops =
  for _ = 1 to num_dummy_loops do app_find_loops app app.lsg done

let construct_cfg app par_loops ppar_loops pppar_loops =
  let n = ref 2 in
  for _ = 1 to par_loops do
    ignore (create_node app.cfg (!n + 1));
    build_connect app 2 (!n + 1);
    n := !n + 1;
    for _ = 1 to ppar_loops do
      let top = !n in
      n := build_straight app !n 1;
      for _ = 1 to pppar_loops do n := build_base_loop app !n done;
      let bottom = build_straight app !n 1 in
      build_connect app !n top;
      n := bottom
    done;
    build_connect app !n 1
  done

let app_main app num_dummy_loops find_loop_iterations par_loops ppar_loops pppar_loops =
  construct_simple_cfg app;
  add_dummy_loops app num_dummy_loops;
  construct_cfg app par_loops ppar_loops pppar_loops;
  (* Performing Loop Recognition, 1 Iteration, then findLoopIteration *)
  app_find_loops app app.lsg;
  for _ = 1 to find_loop_iterations do app_find_loops app (loop_structure_graph ()) done;
  calculate_nesting_level app.lsg;
  (num_loops app.lsg, num_nodes app.cfg)

let run n =
  let loops, nodes = app_main (loop_tester_app ()) n 50 10 10 5 in
  (loops * 1000000) + nodes

let () = Runner.main run 15000
