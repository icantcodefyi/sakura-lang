let pipeline n =
  let xs = List.init n Fun.id in
  let ys = List.map (fun x -> x * 7 mod 1000) xs in
  let zs = List.filter (fun y -> y mod 2 = 0) ys in
  List.fold_left ( + ) 0 zs

type binary = Leaf | Node of binary * binary
let rec bottom_up d =
  if d > 0 then let l = bottom_up (d-1) in let r = bottom_up (d-1) in Node (l,r)
  else Node (Leaf,Leaf)
let rec check = function
  | Leaf -> 0
  | Node (l,r) -> let a = check l in let b = check r in 1+a+b
let binary_trees n =
  let maximum = max 6 n in
  let total = ref (check (bottom_up (maximum+1))) in
  let long_lived = bottom_up maximum in
  let d = ref 4 in
  while !d <= maximum do
    let iterations = 1 lsl (maximum - !d + 4) in
    for _ = 1 to iterations do total := !total + check (bottom_up !d) done;
    d := !d + 2
  done;
  !total + check long_lived

type color = Red | Black
type tree = E | T of color * tree * int * tree
let balance c l k r = match c,l,r with
  | Black,T(Red,T(Red,a,x,b),y,d),_ -> T(Red,T(Black,a,x,b),y,T(Black,d,k,r))
  | Black,T(Red,a,x,T(Red,b,y,d)),_ -> T(Red,T(Black,a,x,b),y,T(Black,d,k,r))
  | Black,_,T(Red,T(Red,b,y,d),z,e) -> T(Red,T(Black,l,k,b),y,T(Black,d,z,e))
  | Black,_,T(Red,b,y,T(Red,d,z,e)) -> T(Red,T(Black,l,k,b),y,T(Black,d,z,e))
  | _ -> T(c,l,k,r)
let rec ins t k = match t with
  | E -> T(Red,E,k,E)
  | T(c,l,y,r) when k<y -> balance c (ins l k) y r
  | T(c,l,y,r) when k>y -> balance c l y (ins r k)
  | _ -> t
let insert t k = match ins t k with T(_,l,y,r) -> T(Black,l,y,r) | E -> E
let rec member t k = match t with E -> false | T(_,l,y,r) -> if k<y then member l k else if k>y then member r k else true
let rec size = function E -> 0 | T(_,l,_,r) -> let a=size l in let b=size r in a+1+b
let rec black_height = function E -> 0 | T(c,l,_,_) -> (if c=Black then 1 else 0) + black_height l
let rbtree n =
  let t = ref E in
  let kept = ref [] in
  for i=0 to n-1 do t := insert !t (i*7919 mod (2*n+1)); if i mod 100=0 then kept := !t :: !kept done;
  let total=ref (size !t) in
  List.iteri (fun j v -> total := !total + black_height v; if member v (j*3) then incr total) (List.rev !kept);
  !total

let get m k = Option.value (Hashtbl.find_opt m k) ~default:0
let map_set n =
  let mi=Hashtbl.create 16 and ms=Hashtbl.create 16 and mt=Hashtbl.create 16 and set=Hashtbl.create 16 in
  for i=0 to n-1 do
    Hashtbl.replace mi i (i mod 1000); Hashtbl.replace ms ("k" ^ string_of_int i) (i mod 1000);
    Hashtbl.replace mt (i mod 100,i/100) (i mod 7); Hashtbl.replace set (i mod 97) ()
  done;
  let snapshot=Hashtbl.copy mi in
  let found=ref 0 in
  for i=0 to 2*n-1 do
    found := !found + get mi i;
    found := !found + get ms ("k" ^ string_of_int i);
    found := !found + get mt (i mod 100,i/100)
  done;
  let i=ref 0 in
  while !i<n do Hashtbl.replace mi !i (get mi !i+7); let k="k" ^ string_of_int !i in Hashtbl.replace ms k (get ms k+7); i := !i+2 done;
  i := 0;
  while !i<n do Hashtbl.remove mi !i; Hashtbl.remove ms ("k" ^ string_of_int !i); Hashtbl.remove mt (!i mod 100,!i/100); i := !i+3 done;
  let total=ref 0 in
  for i=0 to n-1 do total := !total + get mi i done;
  for i=0 to n-1 do total := !total + get ms ("k" ^ string_of_int i) done;
  for i=0 to n-1 do total := !total + get mt (i mod 100,i/100) done;
  let snap=ref 0 in for i=0 to n-1 do snap := !snap + get snapshot i done;
  !found + !total + !snap + Hashtbl.length mi + Hashtbl.length ms + Hashtbl.length mt + Hashtbl.length snapshot + Hashtbl.length set

type shape = Circle of int | Rect of int * int | Square of int
type item = { id: int; shape: shape; tag: int option }
let area = function Circle r -> 3*r*r | Rect(w,h) -> w*h | Square a -> a*a
let grow it =
  let shape = match it.shape with Circle r -> Rect(r,r+1) | Rect(w,h) -> Square((w+h) mod 100) | Square a -> Circle(a mod 50+1) in
  let tag=match it.tag with None -> Some 0 | Some t when t>5 -> None | Some t -> Some(t+1) in
  { it with shape; tag }
let shapes n =
  let seed=ref 42 in
  let items=ref (List.init n (fun id ->
    seed := !seed * 48271 mod 2147483647;
    let k= !seed mod 100 in
    let shape=if k<33 then Circle k else if k<66 then Rect(k,k mod 7+1) else Square(k mod 13) in
    {id;shape;tag=if k mod 4=0 then None else Some(k mod 9)})) in
  let total=ref 0 in
  for _=1 to 5 do
    items := List.map grow !items;
    List.iter (fun it -> total := !total + area it.shape + Option.value it.tag ~default:0 + it.id mod 3) !items
  done;
  !total
