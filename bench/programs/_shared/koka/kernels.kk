module kernels

pub fun pipeline(n: int): int {
  val xs = list(0,n - 1)
  val ys = xs.map(fn(x){ x*7%1000 })
  val zs = ys.filter(fn(y){ y%2==0 })
  zs.foldl(0,fn(acc,z){ acc+z })
}

type binary { Leaf; Node(left: binary, right: binary) }
fun bottom-up(d: int): div binary {
  if (d>0) then {
    val l = bottom-up(d - 1)
    val r = bottom-up(d - 1)
    Node(l,r)
  } else Node(Leaf,Leaf)
}
fun check(t: binary): int {
  match t {
    Leaf -> 0
    Node(l,r) -> { val a=check(l); val b=check(r); 1+a+b }
  }
}
pub fun binary-trees(n: int): div int {
  val maximum = max(6,n)
  var total := check(bottom-up(maximum+1))
  val long-lived = bottom-up(maximum)
  var d := 4
  while(fn(){d<=maximum}, fn(){
    val iterations = 2.pow(maximum - d + 4)
    for(iterations, fn(_){ total := total + check(bottom-up(d)) })
    d := d+2
  })
  total + check(long-lived)
}

type tree { E; T(red: bool, left: tree, key: int, right: tree) }
fun balance(c: bool,l: tree,k: int,r: tree): tree {
  match (c,l,r) {
    (False,T(True,T(True,a,x,b),y,d),_) -> T(True,T(False,a,x,b),y,T(False,d,k,r))
    (False,T(True,a,x,T(True,b,y,d)),_) -> T(True,T(False,a,x,b),y,T(False,d,k,r))
    (False,_,T(True,T(True,b,y,d),z,e)) -> T(True,T(False,l,k,b),y,T(False,d,z,e))
    (False,_,T(True,b,y,T(True,d,z,e))) -> T(True,T(False,l,k,b),y,T(False,d,z,e))
    _ -> T(c,l,k,r)
  }
}
fun ins(t: tree,k: int): tree {
  match t {
    E -> T(True,E,k,E)
    T(c,l,y,r) -> if (k<y) then balance(c,ins(l,k),y,r) else if (k>y) then balance(c,l,y,ins(r,k)) else t
  }
}
fun insert(t: tree,k: int): tree {
  match ins(t,k) { T(_,l,y,r) -> T(False,l,y,r); E -> E }
}
fun member(t: tree,k: int): bool {
  match t { E -> False; T(_,l,y,r) -> if (k<y) then member(l,k) else if (k>y) then member(r,k) else True }
}
fun size(t: tree): int { match t { E -> 0; T(_,l,_,r) -> { val a=size(l); val b=size(r); a+1+b } } }
fun black-height(t: tree): int { match t { E -> 0; T(c,l,_,_) -> (if c then 0 else 1)+black-height(l) } }
pub fun rbtree(n: int): int {
  var t := E
  var kept := []
  for(n,fn(i){
    t := insert(t,i*7919%(2*n+1))
    if (i%100==0) then kept := Cons(t,kept)
  })
  var total := size(t)
  kept.reverse.foreach-indexed(fn(j,v){
    total := total + black-height(v)
    if member(v,j*3) then total := total+1
  })
  total
}

type shape { Circle(radius: int); Rect(width: int,height: int); Square(side: int) }
struct item { id: int; shape: shape; tag: maybe<int> }
fun area(s: shape): int { match s { Circle(r) -> 3*r*r; Rect(w,h) -> w*h; Square(a) -> a*a } }
fun grow(it: item): item {
  val shape = match it.shape { Circle(r) -> Rect(r,r+1); Rect(w,h) -> Square((w+h)%100); Square(a) -> Circle(a%50+1) }
  val tag = match it.tag { Nothing -> Just(0); Just(t) -> if (t>5) then Nothing else Just(t+1) }
  Item(it.id,shape,tag)
}
pub fun shapes(n: int): int {
  var seed := 42
  var items := []
  for(n,fn(i){
    seed := seed*48271%2147483647
    val k=seed%100
    val shape=if (k<33) then Circle(k) else if (k<66) then Rect(k,k%7+1) else Square(k%13)
    val tag=if (k%4==0) then Nothing else Just(k%9)
    items := Cons(Item(i,shape,tag),items)
  })
  items := items.reverse
  var total := 0
  for(5,fn(_){
    items := items.map(grow)
    items.foreach(fn(it){ total := total+area(it.shape)+it.tag.default(0)+it.id%3 })
  })
  total
}

effect tick { fun next(): int }
fun counter(action: () -> <tick|e> a): e a {
  var value := 0
  with handler { fun next() { value := value+1; value } }
  action()
}
fun tick-loop(n: int): tick int {
  var acc := 0
  for(n,fn(_){ acc := acc+next()%7 })
  acc
}
pub fun handled-d1(n: int): int { counter(fn(){tick-loop(n)}) }
pub fun handled-fwd(n: int): int {
  with counter
  with handler { fun next() { next() } }
  tick-loop(n)
}
pub fun handled-reentry(n: int): int {
  with counter
  list(0,n - 1).foldl(0,fn(acc,_){ acc+next()%7 })
}
effect pause { ctl pause(): () }
pub fun handled-suspend(n: int): int {
  var elapsed := 0
  // A resumable operation captures the continuation and resumes it once; no host sleep.
  with handler { ctl pause() { elapsed := elapsed+1; resume(()) } }
  with counter
  var acc := 0
  for(n,fn(i){ acc := acc+next()%7; if (i==n/2) then pause() })
  acc+elapsed
}

effect pad1 { fun pad1(): int }

effect pad2 { fun pad2(): int }

effect pad3 { fun pad3(): int }

effect pad4 { fun pad4(): int }

effect pad5 { fun pad5(): int }

effect pad6 { fun pad6(): int }

effect pad7 { fun pad7(): int }

effect pad8 { fun pad8(): int }

effect pad9 { fun pad9(): int }

effect pad10 { fun pad10(): int }

effect pad11 { fun pad11(): int }

effect pad12 { fun pad12(): int }

effect pad13 { fun pad13(): int }

effect pad14 { fun pad14(): int }

effect pad15 { fun pad15(): int }

pub fun handled-d4(n: int): int {
  with counter
  with handler { fun pad1() { 1 } }
  with handler { fun pad2() { 2 } }
  with handler { fun pad3() { 3 } }
  tick-loop(n)
}

pub fun handled-d16(n: int): int {
  with counter
  with handler { fun pad1() { 1 } }
  with handler { fun pad2() { 2 } }
  with handler { fun pad3() { 3 } }
  with handler { fun pad4() { 4 } }
  with handler { fun pad5() { 5 } }
  with handler { fun pad6() { 6 } }
  with handler { fun pad7() { 7 } }
  with handler { fun pad8() { 8 } }
  with handler { fun pad9() { 9 } }
  with handler { fun pad10() { 10 } }
  with handler { fun pad11() { 11 } }
  with handler { fun pad12() { 12 } }
  with handler { fun pad13() { 13 } }
  with handler { fun pad14() { 14 } }
  with handler { fun pad15() { 15 } }
  tick-loop(n)
}

// The pinned std/data/{map,dict,imap} modules contain declarations only.
// This persistent AVL map implements the same key operations and retained snapshot.
type map-key { KI(i: int); KS(s: string); KT(x: int,y: int) }
fun key-rank(k: map-key): int { match k { KI(_) -> 0; KS(_) -> 1; KT(_,_) -> 2 } }
fun key-cmp(a: map-key,b: map-key): order {
  match (a,b) {
    (KI(x),KI(y)) -> cmp(x,y)
    (KS(x),KS(y)) -> cmp(x,y)
    (KT(x,y),KT(z,w)) -> if (x==z) then cmp(y,w) else cmp(x,z)
    _ -> cmp(key-rank(a),key-rank(b))
  }
}
type pmap { Empty; M(l: pmap,k: map-key,v: int,r: pmap,h: int,count: int) }
fun mh(t: pmap): int { match t { Empty -> 0; M(_,_,_,_,h,_) -> h } }
fun ml(t: pmap): int { match t { Empty -> 0; M(_,_,_,_,_,count) -> count } }
fun mk(l: pmap,k: map-key,v: int,r: pmap): pmap { M(l,k,v,r,1+max(mh(l),mh(r)),ml(l)+1+ml(r)) }
fun mb(l: pmap,k: map-key,v: int,r: pmap): pmap {
  if (mh(l)>mh(r)+1) then {
    match l {
      M(ll,lk,lv,lr,_,_) -> if (mh(ll)>=mh(lr)) then mk(ll,lk,lv,mk(lr,k,v,r)) else {
        match lr { M(a,x,y,b,_,_) -> mk(mk(ll,lk,lv,a),x,y,mk(b,k,v,r)); Empty -> mk(l,k,v,r) }
      }
      Empty -> mk(l,k,v,r)
    }
  } else if (mh(r)>mh(l)+1) then {
    match r {
      M(rl,rk,rv,rr,_,_) -> if (mh(rr)>=mh(rl)) then mk(mk(l,k,v,rl),rk,rv,rr) else {
        match rl { M(a,x,y,b,_,_) -> mk(mk(l,k,v,a),x,y,mk(b,rk,rv,rr)); Empty -> mk(l,k,v,r) }
      }
      Empty -> mk(l,k,v,r)
    }
  } else mk(l,k,v,r)
}
fun put(t: pmap,k: map-key,v: int): pmap {
  match t {
    Empty -> mk(Empty,k,v,Empty)
    M(l,x,y,r,_,_) -> match key-cmp(k,x) {
      Lt -> mb(put(l,k,v),x,y,r)
      Gt -> mb(l,x,y,put(r,k,v))
      Eq -> mk(l,k,v,r)
    }
  }
}
fun get(t: pmap,k: map-key): int {
  match t {
    Empty -> 0
    M(l,x,v,r,_,_) -> match key-cmp(k,x) { Lt -> get(l,k); Gt -> get(r,k); Eq -> v }
  }
}
fun smallest(t: pmap): (map-key,int) {
  match t { M(Empty,k,v,_,_,_) -> (k,v); M(l,_,_,_,_,_) -> smallest(l); Empty -> (KI(0),0) }
}
fun remove(t: pmap,k: map-key): pmap {
  match t {
    Empty -> Empty
    M(l,x,v,r,_,_) -> match key-cmp(k,x) {
      Lt -> mb(remove(l,k),x,v,r)
      Gt -> mb(l,x,v,remove(r,k))
      Eq -> match (l,r) {
        (Empty,_) -> r
        (_,Empty) -> l
        _ -> { val (a,b)=smallest(r); mb(l,a,b,remove(r,a)) }
      }
    }
  }
}
pub fun map-set(n: int): div int {
  var mi := Empty
  var ms := Empty
  var mt := Empty
  var s := Empty
  for(n,fn(i){
    mi := put(mi,KI(i),i%1000)
    ms := put(ms,KS("k" ++ i.show),i%1000)
    mt := put(mt,KT(i%100,i/100),i%7)
    s := put(s,KI(i%97),1)
  })
  val snapshot = mi
  var found := 0
  for(2*n,fn(i){
    found := found+get(mi,KI(i))
    found := found+get(ms,KS("k" ++ i.show))
    found := found+get(mt,KT(i%100,i/100))
  })
  var cursor := 0
  while(fn(){cursor<n},fn(){
    mi := put(mi,KI(cursor),get(mi,KI(cursor))+7)
    ms := put(ms,KS("k" ++ cursor.show),get(ms,KS("k" ++ cursor.show))+7)
    cursor := cursor+2
  })
  cursor := 0
  while(fn(){cursor<n},fn(){
    mi := remove(mi,KI(cursor))
    ms := remove(ms,KS("k" ++ cursor.show))
    mt := remove(mt,KT(cursor%100,cursor/100))
    cursor := cursor+3
  })
  var total := 0
  for(n,fn(i){ total := total+get(mi,KI(i)) })
  for(n,fn(i){ total := total+get(ms,KS("k" ++ i.show)) })
  for(n,fn(i){ total := total+get(mt,KT(i%100,i/100)) })
  var snap := 0
  for(n,fn(i){ snap := snap+get(snapshot,KI(i)) })
  found+total+snap+ml(mi)+ml(ms)+ml(mt)+ml(snapshot)+ml(s)
}
