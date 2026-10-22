(* DeltaBlue from Are We Fast Yet; see bench/programs/deltablue/LICENSE. Matches the JS control:
   stable strongest-first sort of the unsatisfied list, the tests return checksums. Values are Int,
   so ScaleConstraint's backward step is integer division (exact for this corpus, as in Rust/Go). *)

(* SOM's Vector: a growable array; queues walk a head index. *)
module Vector = struct
  type 'a t = { mutable data : 'a array; mutable size : int }

  let create () = { data = [||]; size = 0 }
  let length v = v.size
  let get v i = if i < v.size then v.data.(i) else invalid_arg "Vector.get"

  let push v x =
    if v.size = Array.length v.data then begin
      let data = Array.make (max 4 (2 * v.size)) x in
      Array.blit v.data 0 data 0 v.size;
      v.data <- data
    end;
    v.data.(v.size) <- x;
    v.size <- v.size + 1

  let iter f v = for i = 0 to v.size - 1 do f v.data.(i) done
  let fold f acc v = let r = ref acc in iter (fun x -> r := f !r x) v; !r

  (* Keeps the order of the elements that are not physically [x]. *)
  let remove v x =
    let j = ref 0 in
    for i = 0 to v.size - 1 do
      if v.data.(i) != x then begin
        v.data.(!j) <- v.data.(i);
        incr j
      end
    done;
    v.size <- !j

  let stable_sort cmp v =
    let a = Array.sub v.data 0 v.size in
    Array.stable_sort cmp a;
    Array.blit a 0 v.data 0 v.size
end

(* Strengths by arithmetic value: smaller is stronger. *)
let required = -800
let preferred = -400
let strong_default = -200
let default = 0
let absolute_weakest = 10000
let stronger a b = a < b
let weakest a b = if b > a then b else a

type direction = Unset | Forward | Backward

type variable = {
  mutable value : int;
  constraints : constr Vector.t;
  mutable determined_by : constr option;
  mutable walk_strength : int;
  mutable stay : bool;
  mutable mark : int;
}

and constr = {
  strength : int;
  kind : kind;
  mutable direction : direction; (* binary constraints *)
  mutable satisfied : bool; (* unary constraints *)
}

and kind =
  | Edit of variable
  | Stay of variable
  | Equality of variable * variable
  | Scale of { v1 : variable; v2 : variable; scale : variable; offset : variable }

let variable value =
  { value; constraints = Vector.create (); determined_by = None;
    walk_strength = absolute_weakest; stay = true; mark = 0 }

let is_input c = match c.kind with Edit _ -> true | _ -> false

let is_satisfied c =
  match c.kind with Edit _ | Stay _ -> c.satisfied | _ -> c.direction <> Unset

let ends c =
  match c.kind with
  | Edit v | Stay v -> (v, v)
  | Equality (v1, v2) | Scale { v1; v2; _ } -> (v1, v2)

let output c =
  match c.kind with
  | Edit v | Stay v -> v
  | Equality (v1, v2) | Scale { v1; v2; _ } -> if c.direction = Forward then v2 else v1

(* The primary input of a binary constraint. *)
let input c = let v1, v2 = ends c in if c.direction = Forward then v1 else v2

let inputs_do f c =
  match c.kind with
  | Edit _ | Stay _ -> ()
  | Equality _ -> f (input c)
  | Scale { scale; offset; _ } -> f (input c); f scale; f offset

let inputs_has_one f c = match c.kind with Edit _ | Stay _ -> false | _ -> f (input c)

let attached c =
  match c.kind with
  | Edit v | Stay v -> [ v ]
  | Equality (v1, v2) -> [ v1; v2 ]
  | Scale { v1; v2; scale; offset } -> [ v1; v2; scale; offset ]

let mark_unsatisfied c = c.direction <- Unset; c.satisfied <- false

let add_to_graph c =
  List.iter (fun v -> Vector.push v.constraints c) (attached c);
  mark_unsatisfied c

let remove_from_graph c =
  List.iter
    (fun v ->
      Vector.remove v.constraints c;
      match v.determined_by with Some d when d == c -> v.determined_by <- None | _ -> ())
    (attached c);
  mark_unsatisfied c

let choose_method c mark =
  match c.kind with
  | Edit out | Stay out -> c.satisfied <- out.mark <> mark && stronger c.strength out.walk_strength
  | Equality _ | Scale _ ->
    let v1, v2 = ends c in
    c.direction <-
      (if v1.mark = mark then
         if v2.mark <> mark && stronger c.strength v2.walk_strength then Forward else Unset
       else if v2.mark = mark then
         if v1.mark <> mark && stronger c.strength v1.walk_strength then Backward else Unset
       else if v1.walk_strength > v2.walk_strength then
         if stronger c.strength v1.walk_strength then Backward else Unset
       else if stronger c.strength v2.walk_strength then Forward
       else Unset)

let execute c =
  match c.kind with
  | Edit _ | Stay _ -> ()
  | Equality (v1, v2) -> if c.direction = Forward then v2.value <- v1.value else v1.value <- v2.value
  | Scale { v1; v2; scale; offset } ->
    if c.direction = Forward then v2.value <- (v1.value * scale.value) + offset.value
    else v1.value <- (v2.value - offset.value) / scale.value

let recalculate c =
  let out = output c in
  (match c.kind with
   | Edit _ | Stay _ ->
     out.walk_strength <- c.strength;
     out.stay <- not (is_input c)
   | Equality _ ->
     let ihn = input c in
     out.walk_strength <- weakest c.strength ihn.walk_strength;
     out.stay <- ihn.stay
   | Scale { scale; offset; _ } ->
     let ihn = input c in
     out.walk_strength <- weakest c.strength ihn.walk_strength;
     out.stay <- ihn.stay && scale.stay && offset.stay);
  if out.stay then execute c

let inputs_known c mark =
  not (inputs_has_one (fun v -> not (v.mark = mark || v.stay || Option.is_none v.determined_by)) c)

type planner = { mutable current_mark : int }

let new_mark p = p.current_mark <- p.current_mark + 1; p.current_mark

let constraints_consuming v f =
  Vector.iter
    (fun c ->
      let determining = match v.determined_by with Some d -> d == c | None -> false in
      if (not determining) && is_satisfied c then f c)
    v.constraints

let add_constraints_consuming_to v coll = constraints_consuming v (Vector.push coll)

let rec satisfy c mark p =
  choose_method c mark;
  if is_satisfied c then begin
    inputs_do (fun i -> i.mark <- mark) c;
    let out = output c in
    let overridden = out.determined_by in
    Option.iter mark_unsatisfied overridden;
    out.determined_by <- Some c;
    if not (add_propagate c mark p) then failwith "Cycle encountered";
    out.mark <- mark;
    overridden
  end
  else if c.strength = required then failwith "Could not satisfy a required constraint"
  else None

and incremental_add c p =
  let mark = new_mark p in
  let rec loop = function Some o -> loop (satisfy o mark p) | None -> () in
  loop (satisfy c mark p)

and incremental_remove c p =
  let out = output c in
  mark_unsatisfied c;
  remove_from_graph c;
  Vector.iter (fun u -> incremental_add u p) (remove_propagate_from out)

and add_propagate c mark p =
  let todo = Vector.create () in
  Vector.push todo c;
  let rec go head =
    if head >= Vector.length todo then true
    else
      let d = Vector.get todo head in
      if (output d).mark = mark then (incremental_remove c p; false)
      else begin
        recalculate d;
        add_constraints_consuming_to (output d) todo;
        go (head + 1)
      end
  in
  go 0

and remove_propagate_from out =
  let unsatisfied = Vector.create () in
  out.determined_by <- None;
  out.walk_strength <- absolute_weakest;
  out.stay <- true;
  let todo = Vector.create () in
  Vector.push todo out;
  let head = ref 0 in
  while !head < Vector.length todo do
    let v = Vector.get todo !head in
    incr head;
    Vector.iter (fun c -> if not (is_satisfied c) then Vector.push unsatisfied c) v.constraints;
    constraints_consuming v (fun c -> recalculate c; Vector.push todo (output c))
  done;
  Vector.stable_sort (fun c1 c2 -> compare c1.strength c2.strength) unsatisfied;
  unsatisfied

let add_constraint p strength kind =
  let c = { strength; kind; direction = Unset; satisfied = false } in
  add_to_graph c;
  incremental_add c p;
  c

let destroy_constraint c p =
  if is_satisfied c then incremental_remove c p;
  remove_from_graph c

let make_plan p sources =
  let mark = new_mark p in
  let plan = Vector.create () in
  let todo = sources in
  let head = ref 0 in
  while !head < Vector.length todo do
    let c = Vector.get todo !head in
    incr head;
    if (output c).mark <> mark && inputs_known c mark then begin
      Vector.push plan c;
      (output c).mark <- mark;
      add_constraints_consuming_to (output c) todo
    end
  done;
  plan

let extract_plan_from_constraints p constraints =
  let sources = Vector.create () in
  List.iter (fun c -> if is_input c && is_satisfied c then Vector.push sources c) constraints;
  make_plan p sources

let change p v new_value =
  let edit = add_constraint p preferred (Edit v) in
  let plan = extract_plan_from_constraints p [ edit ] in
  for _ = 1 to 10 do
    v.value <- new_value;
    Vector.iter execute plan
  done;
  destroy_constraint edit p

let chain_test n =
  let p = { current_mark = 1 } in
  let vars = Array.init (n + 1) (fun _ -> variable 0) in
  for i = 0 to n - 1 do
    ignore (add_constraint p required (Equality (vars.(i), vars.(i + 1))))
  done;
  ignore (add_constraint p strong_default (Stay vars.(n)));
  let edit = add_constraint p preferred (Edit vars.(0)) in
  let plan = extract_plan_from_constraints p [ edit ] in
  let sum = ref 0 in
  for i = 0 to 99 do
    vars.(0).value <- i;
    Vector.iter execute plan;
    if vars.(n).value <> i then failwith "Chain test failed!";
    sum := !sum + vars.(n).value
  done;
  destroy_constraint edit p;
  !sum + Vector.length plan

let projection_test n =
  if n = 0 then 0
  else begin
    let p = { current_mark = 1 } in
    let dests = Vector.create () in
    let scale = variable 10 and offset = variable 1000 in
    let src = ref (variable 0) and dst = ref (variable 0) in
    for i = 1 to n do
      src := variable i;
      dst := variable i;
      Vector.push dests !dst;
      ignore (add_constraint p default (Stay !src));
      ignore (add_constraint p required (Scale { v1 = !src; v2 = !dst; scale; offset }))
    done;
    let check k base =
      for i = 0 to n - 2 do
        if (Vector.get dests i).value <> ((i + 1) * 5) + base then
          failwith (Printf.sprintf "Projection test %d failed!" k)
      done
    in
    let total () = Vector.fold (fun acc d -> acc + d.value) 0 dests in
    change p !src 17;
    if !dst.value <> 1170 then failwith "Projection test 1 failed!";
    change p !dst 1050;
    if !src.value <> 5 then failwith "Projection test 2 failed!";
    change p scale 5;
    check 3 1000;
    let c = total () in
    change p offset 2000;
    check 4 2000;
    1170 + 5 + c + total ()
  end

let deltablue n = chain_test n + projection_test n

let () = Runner.main deltablue 10000
