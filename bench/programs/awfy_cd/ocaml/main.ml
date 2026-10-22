(* CD from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/cd.js and
   Java/src/cd; Copyright (c) 2001-2010, Purdue University; Copyright (C) 2015 Apple Inc.; see
   ../LICENSE). AWFY's RedBlackTree is a parent-pointer tree mutated in place: a functor over the
   key's compare_to (compareTo), nodes a variant with an inline mutable record, Nil for null,
   identity physical equality (==). SOM's Vector is a doubling array (append, size, at,
   iteration). sin and cos are libm's. Every product is bound with `let` before it is added, as
   ocamlopt on arm64 would otherwise fuse it into fmadd/fmsub. run n is cd (n - n mod 2): AWFY's
   Simulator reads aircraft i + 1 for every even i, so it is defined for even n only. *)

(* ---- som: Vector *)

module Vector = struct
  type 'a t = { mutable storage : 'a array; mutable last : int }

  let create () = { storage = [||]; last = 0 }

  let append v elem =
    if Array.length v.storage = 0 then v.storage <- Array.make 10 elem
    else if v.last >= Array.length v.storage then begin
      let storage = Array.make (2 * Array.length v.storage) elem in
      Array.blit v.storage 0 storage 0 v.last;
      v.storage <- storage
    end;
    v.storage.(v.last) <- elem;
    v.last <- v.last + 1

  let size v = v.last
  let at v i = v.storage.(i)

  let for_each v f =
    for i = 0 to v.last - 1 do
      f v.storage.(i)
    done
end

(* ---- vectors *)

type vector2d = { x : float; y : float }
type vector3d = { x3 : float; y3 : float; z3 : float }

let plus2 a b = { x = a.x +. b.x; y = a.y +. b.y }
let minus2 a b = { x = a.x -. b.x; y = a.y -. b.y }

let compare_numbers (a : float) (b : float) =
  if a = b then 0
  else if a < b then -1
  else if a > b then 1
  (* We say that NaN is smaller than non-NaN. *)
  else if a = a then 1
  else -1

let plus3 a b = { x3 = a.x3 +. b.x3; y3 = a.y3 +. b.y3; z3 = a.z3 +. b.z3 }
let minus3 a b = { x3 = a.x3 -. b.x3; y3 = a.y3 -. b.y3; z3 = a.z3 -. b.z3 }
let dot a b =
  let xx = a.x3 *. b.x3 and yy = a.y3 *. b.y3 and zz = a.z3 *. b.z3 in
  xx +. yy +. zz
let squared_magnitude a = dot a a
let magnitude a = Float.sqrt (squared_magnitude a)
let times a amount = { x3 = a.x3 *. amount; y3 = a.y3 *. amount; z3 = a.z3 *. amount }

(* ---- red-black tree *)

module type KEY = sig
  type t

  val compare_to : t -> t -> int
end

module RedBlackTree (K : KEY) = struct
  type 'v node =
    | Nil
    | N of {
        key : K.t;
        mutable value : 'v;
        mutable left : 'v node;
        mutable right : 'v node;
        mutable parent : 'v node;
        mutable red : bool;
      }

  type 'v t = { mutable root : 'v node }

  let create () = { root = Nil }
  let left = function N r -> r.left | Nil -> invalid_arg "left of nil"
  let right = function N r -> r.right | Nil -> invalid_arg "right of nil"
  let parent = function N r -> r.parent | Nil -> invalid_arg "parent of nil"
  let is_red = function N r -> r.red | Nil -> false
  let set_left n v = match n with N r -> r.left <- v | Nil -> invalid_arg "set_left"
  let set_right n v = match n with N r -> r.right <- v | Nil -> invalid_arg "set_right"
  let set_parent n v = match n with N r -> r.parent <- v | Nil -> invalid_arg "set_parent"
  let set_red n v = match n with N r -> r.red <- v | Nil -> invalid_arg "set_red"

  let rec tree_minimum x = match left x with Nil -> x | l -> tree_minimum l

  let successor node =
    if right node != Nil then tree_minimum (right node)
    else begin
      let x = ref node and y = ref (parent node) in
      while !y != Nil && !x == right !y do
        x := !y;
        y := parent !y
      done;
      !y
    end

  let find_node t key =
    let rec go current =
      match current with
      | Nil -> Nil
      | N r ->
          let comparison = K.compare_to key r.key in
          if comparison = 0 then current else if comparison < 0 then go r.left else go r.right
    in
    go t.root

  let get t key = match find_node t key with N r -> Some r.value | Nil -> None

  type 'v insert_result = Inserted of 'v node | Replaced of 'v

  let tree_insert t key value =
    let rec go y x =
      match x with
      | N r ->
          let comparison = K.compare_to key r.key in
          if comparison < 0 then go x r.left
          else if comparison > 0 then go x r.right
          else begin
            let old = r.value in
            r.value <- value;
            Replaced old
          end
      | Nil ->
          let z = N { key; value; left = Nil; right = Nil; parent = y; red = true } in
          (match y with
          | Nil -> t.root <- z
          | N r -> if K.compare_to key r.key < 0 then r.left <- z else r.right <- z);
          Inserted z
    in
    go Nil t.root

  let left_rotate t x =
    let y = right x in
    (* Turn y's left subtree into x's right subtree. *)
    set_right x (left y);
    if left y != Nil then set_parent (left y) x;
    (* Link x's parent to y. *)
    set_parent y (parent x);
    if parent x == Nil then t.root <- y
    else if x == left (parent x) then set_left (parent x) y
    else set_right (parent x) y;
    (* Put x on y's left. *)
    set_left y x;
    set_parent x y

  let right_rotate t y =
    let x = left y in
    (* Turn x's right subtree into y's left subtree. *)
    set_left y (right x);
    if right x != Nil then set_parent (right x) y;
    (* Link y's parent to x. *)
    set_parent x (parent y);
    if parent y == Nil then t.root <- x
    else if y == left (parent y) then set_left (parent y) x
    else set_right (parent y) x;
    set_right x y;
    set_parent y x

  (* AWFY's put: the replaced value, or None after inserting a new entry. *)
  let put t key value =
    match tree_insert t key value with
    | Replaced old -> Some old
    | Inserted z ->
        let x = ref z in
        while !x != t.root && is_red (parent !x) do
          if parent !x == left (parent (parent !x)) then begin
            let y = right (parent (parent !x)) in
            if is_red y then begin
              (* Case 1 *)
              set_red (parent !x) false;
              set_red y false;
              set_red (parent (parent !x)) true;
              x := parent (parent !x)
            end
            else begin
              if !x == right (parent !x) then begin
                (* Case 2 *)
                x := parent !x;
                left_rotate t !x
              end;
              (* Case 3 *)
              set_red (parent !x) false;
              set_red (parent (parent !x)) true;
              right_rotate t (parent (parent !x))
            end
          end
          else begin
            (* Same as "then" clause with "right" and "left" exchanged. *)
            let y = left (parent (parent !x)) in
            if is_red y then begin
              (* Case 1 *)
              set_red (parent !x) false;
              set_red y false;
              set_red (parent (parent !x)) true;
              x := parent (parent !x)
            end
            else begin
              if !x == left (parent !x) then begin
                (* Case 2 *)
                x := parent !x;
                right_rotate t !x
              end;
              (* Case 3 *)
              set_red (parent !x) false;
              set_red (parent (parent !x)) true;
              left_rotate t (parent (parent !x))
            end
          end
        done;
        set_red t.root false;
        None

  let remove_fixup t x x_parent =
    let x = ref x and x_parent = ref x_parent in
    while !x != t.root && not (is_red !x) do
      if !x == left !x_parent then begin
        (* The text points out that w cannot be nil: it follows from the red-black properties. *)
        let w = ref (right !x_parent) in
        if is_red !w then begin
          (* Case 1 *)
          set_red !w false;
          set_red !x_parent true;
          left_rotate t !x_parent;
          w := right !x_parent
        end;
        if (not (is_red (left !w))) && not (is_red (right !w)) then begin
          (* Case 2 *)
          set_red !w true;
          x := !x_parent;
          x_parent := parent !x
        end
        else begin
          if not (is_red (right !w)) then begin
            (* Case 3 *)
            set_red (left !w) false;
            set_red !w true;
            right_rotate t !w;
            w := right !x_parent
          end;
          (* Case 4 *)
          set_red !w (is_red !x_parent);
          set_red !x_parent false;
          if right !w != Nil then set_red (right !w) false;
          left_rotate t !x_parent;
          x := t.root;
          x_parent := parent !x
        end
      end
      else begin
        (* Same as "then" clause with "right" and "left" exchanged. *)
        let w = ref (left !x_parent) in
        if is_red !w then begin
          (* Case 1 *)
          set_red !w false;
          set_red !x_parent true;
          right_rotate t !x_parent;
          w := left !x_parent
        end;
        if (not (is_red (right !w))) && not (is_red (left !w)) then begin
          (* Case 2 *)
          set_red !w true;
          x := !x_parent;
          x_parent := parent !x
        end
        else begin
          if not (is_red (left !w)) then begin
            (* Case 3 *)
            set_red (right !w) false;
            set_red !w true;
            left_rotate t !w;
            w := left !x_parent
          end;
          (* Case 4 *)
          set_red !w (is_red !x_parent);
          set_red !x_parent false;
          if left !w != Nil then set_red (left !w) false;
          right_rotate t !x_parent;
          x := t.root;
          x_parent := parent !x
        end
      end
    done;
    if !x != Nil then set_red !x false

  (* AWFY's remove: the removed value, if the key was present. *)
  let remove t key =
    match find_node t key with
    | Nil -> None
    | N zr as z ->
        (* Y is the node to be unlinked from the tree. *)
        let y = if zr.left == Nil || zr.right == Nil then z else successor z in
        (* X is the child of y which might potentially replace y in the tree; it may be Nil. *)
        let x = if left y != Nil then left y else right y in
        let x_parent = parent y in
        if x != Nil then set_parent x (parent y);
        if parent y == Nil then t.root <- x
        else if y == left (parent y) then set_left (parent y) x
        else set_right (parent y) x;
        if y != z then begin
          if not (is_red y) then remove_fixup t x x_parent;
          set_parent y zr.parent;
          set_red y zr.red;
          set_left y zr.left;
          set_right y zr.right;
          if zr.left != Nil then set_parent zr.left y;
          if zr.right != Nil then set_parent zr.right y;
          if zr.parent != Nil then begin
            if left zr.parent == z then set_left zr.parent y else set_right zr.parent y
          end
          else t.root <- y
        end
        else if not (is_red y) then remove_fixup t x x_parent;
        Some zr.value

  (* AWFY's forEach: every entry in key order. *)
  let for_each t f =
    if t.root != Nil then begin
      let current = ref (tree_minimum t.root) in
      while !current != Nil do
        (match !current with N r -> f r.key r.value | Nil -> ());
        current := successor !current
      done
    end
end

(* ---- the simulation *)

module VoxelTree = RedBlackTree (struct
  type t = vector2d

  let compare_to a b =
    let result = compare_numbers a.x b.x in
    if result <> 0 then result else compare_numbers a.y b.y
end)

(* A CallSign is its int. *)
module CallSignTree = RedBlackTree (struct
  type t = int

  let compare_to (a : int) b = if a = b then 0 else if a < b then -1 else 1
end)

let min_x = 0.0
let min_y = 0.0
let max_x = 1000.0
let max_y = 1000.0
let min_z = 0.0
let max_z = 10.0
let proximity_radius = 1.0
let good_voxel_size = proximity_radius *. 2.0

type motion = { callsign : int; pos_one : vector3d; pos_two : vector3d }
type collision = { aircraft_a : int; aircraft_b : int; position : vector3d }
type aircraft = { sign : int; at : vector3d }

let delta m = minus3 m.pos_two m.pos_one

let find_intersection m1 m2 =
  let init1 = m1.pos_one and init2 = m2.pos_one in
  let vec1 = delta m1 and vec2 = delta m2 in
  let radius = proximity_radius in
  (* a = (V2 - V1)^T * (V2 - V1) *)
  let a = squared_magnitude (minus3 vec2 vec1) in
  if a <> 0.0 then begin
    (* b = 2 * <I1-I2, V1-V2> *)
    let b = 2.0 *. dot (minus3 init1 init2) (minus3 vec1 vec2) in
    (* c = -r^2 + (I2 - I1)^T * (I2 - I1) *)
    let r2 = -.radius *. radius in
    let c = r2 +. squared_magnitude (minus3 init2 init1) in
    let bb = b *. b and ac4 = 4.0 *. a *. c in
    let discr = bb -. ac4 in
    if discr < 0.0 then None
    else begin
      let v1 = (-.b -. Float.sqrt discr) /. (2.0 *. a) in
      let v2 = (-.b +. Float.sqrt discr) /. (2.0 *. a) in
      if
        v1 <= v2
        && ((v1 <= 1.0 && 1.0 <= v2) || (v1 <= 0.0 && 0.0 <= v2) || (0.0 <= v1 && v2 <= 1.0))
      then begin
        (* Pick a good "time" at which to report the collision. *)
        let v = if v1 <= 0.0 then 0.0 else v1 in
        let result1 = plus3 init1 (times vec1 v) in
        let result2 = plus3 init2 (times vec2 v) in
        let result = times (plus3 result1 result2) 0.5 in
        if
          result.x3 >= min_x && result.x3 <= max_x && result.y3 >= min_y && result.y3 <= max_y
          && result.z3 >= min_z && result.z3 <= max_z
        then Some result
        else None
      end
      else None
    end
  end
  else begin
    (* The planes have the same speeds and are moving in parallel (or not moving at all). *)
    let dist = magnitude (minus3 init2 init1) in
    if dist <= radius then Some (times (plus3 init1 init2) 0.5) else None
  end

let is_in_voxel voxel motion =
  if voxel.x > max_x || voxel.x < min_x || voxel.y > max_y || voxel.y < min_y then false
  else begin
    let init = motion.pos_one and fin = motion.pos_two in
    let v_s = good_voxel_size in
    let r = proximity_radius /. 2.0 in
    let v_x = voxel.x and x0 = init.x3 and xv = fin.x3 -. init.x3 in
    let v_y = voxel.y and y0 = init.y3 and yv = fin.y3 -. init.y3 in
    let low_x = (v_x -. r -. x0) /. xv and high_x = (v_x +. v_s +. r -. x0) /. xv in
    let low_x, high_x = if xv < 0.0 then (high_x, low_x) else (low_x, high_x) in
    let low_y = (v_y -. r -. y0) /. yv and high_y = (v_y +. v_s +. r -. y0) /. yv in
    let low_y, high_y = if yv < 0.0 then (high_y, low_y) else (low_y, high_y) in
    ((xv = 0.0 && v_x <= x0 +. r && x0 -. r <= v_x +. v_s) (* no motion in x *)
    || (low_x <= 1.0 && 1.0 <= high_x)
    || (low_x <= 0.0 && 0.0 <= high_x)
    || (0.0 <= low_x && high_x <= 1.0))
    && ((yv = 0.0 && v_y <= y0 +. r && y0 -. r <= v_y +. v_s) (* no motion in y *)
       || (low_y <= 1.0 && 1.0 <= high_y)
       || (low_y <= 0.0 && 0.0 <= high_y)
       || (0.0 <= low_y && high_y <= 1.0))
    && (xv = 0.0 || yv = 0.0 (* no motion in x or y or both *)
       || (low_y <= high_x && high_x <= high_y)
       || (low_y <= low_x && low_x <= high_y)
       || (low_x <= low_y && high_y <= high_x))
  end

let put_into_map voxel_map voxel motion =
  let vec =
    match VoxelTree.get voxel_map voxel with
    | Some vec -> vec
    | None ->
        let vec = Vector.create () in
        ignore (VoxelTree.put voxel_map voxel vec);
        vec
  in
  Vector.append vec motion

let horizontal = { x = good_voxel_size; y = 0.0 }
let vertical = { x = 0.0; y = good_voxel_size }

let rec recurse voxel_map seen next_voxel motion =
  if is_in_voxel next_voxel motion && VoxelTree.put seen next_voxel true <> Some true then begin
    put_into_map voxel_map next_voxel motion;
    recurse voxel_map seen (minus2 next_voxel horizontal) motion;
    recurse voxel_map seen (plus2 next_voxel horizontal) motion;
    recurse voxel_map seen (minus2 next_voxel vertical) motion;
    recurse voxel_map seen (plus2 next_voxel vertical) motion;
    recurse voxel_map seen (minus2 (minus2 next_voxel horizontal) vertical) motion;
    recurse voxel_map seen (plus2 (minus2 next_voxel horizontal) vertical) motion;
    recurse voxel_map seen (minus2 (plus2 next_voxel horizontal) vertical) motion;
    recurse voxel_map seen (plus2 (plus2 next_voxel horizontal) vertical) motion
  end

let voxel_hash position =
  let x_div = Float.to_int (position.x3 /. good_voxel_size) in
  let y_div = Float.to_int (position.y3 /. good_voxel_size) in
  let x = good_voxel_size *. Float.of_int x_div and y = good_voxel_size *. Float.of_int y_div in
  let x = if position.x3 < 0.0 then x -. good_voxel_size else x in
  let y = if position.y3 < 0.0 then y -. good_voxel_size else y in
  { x; y }

let draw_motion_on_voxel_map voxel_map motion =
  let seen = VoxelTree.create () in
  recurse voxel_map seen (voxel_hash motion.pos_one) motion

let reduce_collision_set motions =
  let voxel_map = VoxelTree.create () in
  Vector.for_each motions (fun motion -> draw_motion_on_voxel_map voxel_map motion);
  let result = Vector.create () in
  VoxelTree.for_each voxel_map (fun _ vec -> if Vector.size vec > 1 then Vector.append result vec);
  result

let handle_new_frame state frame =
  let motions = Vector.create () in
  let seen = CallSignTree.create () in
  Vector.for_each frame (fun aircraft ->
      let old_position = CallSignTree.put state aircraft.sign aircraft.at in
      let new_position = aircraft.at in
      ignore (CallSignTree.put seen aircraft.sign true);
      (* Treat newly introduced aircraft as if they were stationary. *)
      let old_position = match old_position with Some p -> p | None -> new_position in
      Vector.append motions { callsign = aircraft.sign; pos_one = old_position; pos_two = new_position });
  (* Remove aircraft that are no longer present. *)
  let to_remove = Vector.create () in
  CallSignTree.for_each state (fun callsign _ ->
      if CallSignTree.get seen callsign <> Some true then Vector.append to_remove callsign);
  Vector.for_each to_remove (fun callsign -> ignore (CallSignTree.remove state callsign));
  let all_reduced = reduce_collision_set motions in
  let collisions = Vector.create () in
  Vector.for_each all_reduced (fun reduced ->
      for i = 0 to Vector.size reduced - 1 do
        let motion1 = Vector.at reduced i in
        for j = i + 1 to Vector.size reduced - 1 do
          let motion2 = Vector.at reduced j in
          match find_intersection motion1 motion2 with
          | Some position ->
              Vector.append collisions
                { aircraft_a = motion1.callsign; aircraft_b = motion2.callsign; position }
          | None -> ()
        done
      done);
  collisions

let simulate callsigns time =
  let frame = Vector.create () in
  let i = ref 0 in
  while !i < Vector.size callsigns do
    Vector.append frame
      {
        sign = Vector.at callsigns !i;
        at = (let c2 = cos time *. 2.0 in { x3 = time; y3 = c2 +. Float.of_int (!i * 3); z3 = 10.0 });
      };
    Vector.append frame
      {
        sign = Vector.at callsigns (!i + 1);
        at = (let s2 = sin time *. 2.0 in { x3 = time; y3 = s2 +. Float.of_int (!i * 3); z3 = 10.0 });
      };
    i := !i + 2
  done;
  frame

let cd num_aircrafts =
  let num_frames = 200 in
  let callsigns = Vector.create () in
  for i = 0 to num_aircrafts - 1 do
    Vector.append callsigns i
  done;
  let state = CallSignTree.create () in
  let actual_collisions = ref 0 in
  for i = 0 to num_frames - 1 do
    let time = Float.of_int i /. 10.0 in
    let collisions = handle_new_frame state (simulate callsigns time) in
    actual_collisions := !actual_collisions + Vector.size collisions
  done;
  !actual_collisions

let run n = cd (n - (n mod 2))
let () = Runner.main run 1000
