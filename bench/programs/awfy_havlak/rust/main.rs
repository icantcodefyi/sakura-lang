// Havlak loop recognition from Are We Fast Yet (smarr/are-we-fast-yet@74306fec
// benchmarks/JavaScript/havlak.js and Java/src/havlak; Apache-2.0, Copyright 2011 Google Inc.;
// the som collections MIT, Copyright (c) 2015-2016 Stefan Marr; see ../LICENSE). run(n) is
// LoopTesterApp.main(n, 50, 10, 10, 5) as loops * 1_000_000 + nodes.
// The object graph is cyclic (in/out edges, loop parents and children, union-find parents), so
// objects live in arenas and refer to each other by index, as the Sakura source and the DeltaBlue
// ports do: blocks in the CFG, loops in the loop structure graph, union-find nodes in the finder
// (index = DFS number), dictionary entries in the dictionary. Mutation is in place. som's Vector
// is a `Vec` (which doubles) with a first index; Set and IdentitySet are one type (identity is
// the index); IdentityDictionary is som's chained hash table keyed by the block's index with the
// block's customHash (its name). forEach is a loop where a closure would borrow the finder twice.

const INITIAL_SIZE: usize = 10;
const INITIAL_CAPACITY: usize = 16;
const UNVISITED: usize = 2147483647; // Marker for uninitialized nodes.
const MAXNONBACKPREDS: usize = 32 * 1024; // Safeguard against pathological algorithm behavior.

// ---- som: Vector

struct Vector<E> {
    storage: Vec<E>,
    first_idx: usize,
}

impl<E> Vector<E> {
    fn new() -> Self {
        Vector {
            storage: Vec::new(),
            first_idx: 0,
        }
    }

    fn get(&self, idx: usize) -> &E {
        &self.storage[idx]
    }

    fn get_mut(&mut self, idx: usize) -> &mut E {
        &mut self.storage[idx]
    }

    fn append(&mut self, elem: E) {
        if self.storage.capacity() == 0 {
            self.storage.reserve_exact(INITIAL_SIZE);
        }
        self.storage.push(elem);
    }

    fn is_empty(&self) -> bool {
        self.storage.len() == self.first_idx
    }

    fn size(&self) -> usize {
        self.storage.len() - self.first_idx
    }

    fn live(&self) -> &[E] {
        &self.storage[self.first_idx..]
    }

    fn has_some(&self, f: impl Fn(&E) -> bool) -> bool {
        self.live().iter().any(f)
    }

    fn remove_all(&mut self) {
        self.first_idx = 0;
        self.storage = Vec::with_capacity(self.storage.capacity());
    }
}

impl<E: Copy> Vector<E> {
    /// som's `removeFirst` on a non-empty vector.
    fn remove_first(&mut self) -> E {
        self.first_idx += 1;
        self.storage[self.first_idx - 1]
    }
}

impl<E> Vector<Option<E>> {
    /// som's `at` on the block map: None past the end.
    fn at(&self, idx: usize) -> Option<&E> {
        self.storage.get(idx).and_then(Option::as_ref)
    }

    fn at_put(&mut self, idx: usize, val: E) {
        while self.storage.len() <= idx {
            self.append(None);
        }
        self.storage[idx] = Some(val);
    }
}

// ---- som: Set and IdentitySet

struct Set<E> {
    items: Vector<E>,
}

impl<E: Copy + PartialEq> Set<E> {
    fn new() -> Self {
        Set {
            items: Vector::new(),
        }
    }

    fn size(&self) -> usize {
        self.items.size()
    }

    fn contains(&self, obj: E) -> bool {
        self.items.has_some(|&e| e == obj)
    }

    fn add(&mut self, obj: E) {
        if !self.contains(obj) {
            self.items.append(obj);
        }
    }
}

// ---- som: IdentityDictionary from a basic block (its index) to an index

struct DictEntry {
    hash: u32,
    key: usize,
    value: usize,
    next: Option<usize>,
}

struct IdentityDictionary {
    buckets: Vec<Option<usize>>,
    entries: Vec<DictEntry>,
    size: usize,
}

fn hash_fn(custom_hash: u32) -> u32 {
    custom_hash ^ (custom_hash >> 16)
}

impl IdentityDictionary {
    fn new() -> Self {
        IdentityDictionary {
            buckets: vec![None; INITIAL_CAPACITY],
            entries: Vec::new(),
            size: 0,
        }
    }

    fn bucket_idx(&self, hash: u32) -> usize {
        ((self.buckets.len() - 1) as u32 & hash) as usize
    }

    fn matches(&self, e: usize, hash: u32, key: usize) -> bool {
        self.entries[e].hash == hash && self.entries[e].key == key
    }

    fn at(&self, key: usize, custom_hash: u32) -> Option<usize> {
        let hash = hash_fn(custom_hash);
        let mut e = self.buckets[self.bucket_idx(hash)];
        while let Some(i) = e {
            if self.matches(i, hash, key) {
                return Some(self.entries[i].value);
            }
            e = self.entries[i].next;
        }
        None
    }

    fn new_entry(&mut self, key: usize, value: usize, hash: u32) -> usize {
        self.entries.push(DictEntry {
            hash,
            key,
            value,
            next: None,
        });
        self.entries.len() - 1
    }

    fn at_put(&mut self, key: usize, custom_hash: u32, value: usize) {
        let hash = hash_fn(custom_hash);
        let i = self.bucket_idx(hash);
        match self.buckets[i] {
            None => {
                self.buckets[i] = Some(self.new_entry(key, value, hash));
                self.size += 1;
            }
            Some(current) => self.insert_bucket_entry(key, value, hash, current),
        }
        if self.size > self.buckets.len() {
            self.resize();
        }
    }

    fn insert_bucket_entry(&mut self, key: usize, value: usize, hash: u32, head: usize) {
        let mut current = head;
        loop {
            if self.matches(current, hash, key) {
                self.entries[current].value = value;
                return;
            }
            match self.entries[current].next {
                None => {
                    self.size += 1;
                    let e = self.new_entry(key, value, hash);
                    self.entries[current].next = Some(e);
                    return;
                }
                Some(next) => current = next,
            }
        }
    }

    fn resize(&mut self) {
        let new_length = self.buckets.len() * 2;
        let old_storage = std::mem::replace(&mut self.buckets, vec![None; new_length]);
        self.transfer_entries(old_storage);
    }

    fn transfer_entries(&mut self, mut old_storage: Vec<Option<usize>>) {
        for i in 0..old_storage.len() {
            if let Some(current) = old_storage[i] {
                old_storage[i] = None;
                if self.entries[current].next.is_none() {
                    let idx =
                        (self.entries[current].hash & (self.buckets.len() - 1) as u32) as usize;
                    self.buckets[idx] = Some(current);
                } else {
                    self.split_bucket(old_storage.len(), i, current);
                }
            }
        }
    }

    fn split_bucket(&mut self, old_length: usize, i: usize, head: usize) {
        let (mut lo_head, mut hi_head) = (None, None);
        let (mut lo_tail, mut hi_tail): (Option<usize>, Option<usize>) = (None, None);
        let mut current = Some(head);
        while let Some(c) = current {
            if self.entries[c].hash & old_length as u32 == 0 {
                match lo_tail {
                    None => lo_head = Some(c),
                    Some(t) => self.entries[t].next = Some(c),
                }
                lo_tail = Some(c);
            } else {
                match hi_tail {
                    None => hi_head = Some(c),
                    Some(t) => self.entries[t].next = Some(c),
                }
                hi_tail = Some(c);
            }
            current = self.entries[c].next;
        }
        if let Some(t) = lo_tail {
            self.entries[t].next = None;
            self.buckets[i] = lo_head;
        }
        if let Some(t) = hi_tail {
            self.entries[t].next = None;
            self.buckets[i + old_length] = hi_head;
        }
    }

    fn remove_all(&mut self) {
        self.buckets = vec![None; self.buckets.len()];
        self.entries = Vec::new();
        self.size = 0;
    }
}

// ---- BasicBlock, BasicBlockEdge, ControlFlowGraph

struct BasicBlock {
    in_edges: Vector<usize>,
    out_edges: Vector<usize>,
    name: usize,
}

impl BasicBlock {
    fn num_pred(&self) -> usize {
        self.in_edges.size()
    }

    fn custom_hash(&self) -> u32 {
        self.name as u32
    }
}

#[allow(dead_code)] // AWFY keeps the edge objects, unread
struct BasicBlockEdge {
    from: usize,
    to: usize,
}

struct ControlFlowGraph {
    basic_block_map: Vector<Option<usize>>,
    start_node: Option<usize>,
    edge_list: Vector<BasicBlockEdge>,
    blocks: Vec<BasicBlock>,
}

impl ControlFlowGraph {
    fn new() -> Self {
        ControlFlowGraph {
            basic_block_map: Vector::new(),
            start_node: None,
            edge_list: Vector::new(),
            blocks: Vec::new(),
        }
    }

    fn custom_hash(&self, bb: usize) -> u32 {
        self.blocks[bb].custom_hash()
    }

    fn create_node(&mut self, name: usize) -> usize {
        let node = match self.basic_block_map.at(name) {
            Some(&node) => node,
            None => {
                self.blocks.push(BasicBlock {
                    in_edges: Vector::new(),
                    out_edges: Vector::new(),
                    name,
                });
                let node = self.blocks.len() - 1;
                self.basic_block_map.at_put(name, node);
                node
            }
        };
        if self.num_nodes() == 1 {
            self.start_node = Some(node);
        }
        node
    }

    /// `new BasicBlockEdge(cfg, fromName, toName)`.
    fn basic_block_edge(&mut self, from_name: usize, to_name: usize) {
        let from = self.create_node(from_name);
        let to = self.create_node(to_name);
        self.blocks[from].out_edges.append(to);
        self.blocks[to].in_edges.append(from);
        self.edge_list.append(BasicBlockEdge { from, to });
    }

    fn num_nodes(&self) -> usize {
        self.basic_block_map.size()
    }
}

// ---- SimpleLoop, LoopStructureGraph

#[allow(dead_code)] // AWFY sets these and never reads them back
struct SimpleLoop {
    basic_blocks: Set<usize>,
    children: Set<usize>,
    parent: Option<usize>,
    header: Option<usize>,
    is_reducible: bool,
    is_root: bool,
    nesting_level: usize,
    counter: usize,
    depth_level: usize,
}

impl SimpleLoop {
    fn new(bb: Option<usize>, is_reducible: bool) -> Self {
        let mut loop_ = SimpleLoop {
            basic_blocks: Set::new(),
            children: Set::new(),
            parent: None,
            header: bb,
            is_reducible,
            is_root: false,
            nesting_level: 0,
            counter: 0,
            depth_level: 0,
        };
        if let Some(bb) = bb {
            loop_.basic_blocks.add(bb);
        }
        loop_
    }

    fn set_nesting_level(&mut self, level: usize) {
        self.nesting_level = level;
        if level == 0 {
            self.is_root = true;
        }
    }
}

struct LoopStructureGraph {
    root: usize,
    loops: Vector<usize>,
    loop_counter: usize,
    heap: Vec<SimpleLoop>,
}

impl LoopStructureGraph {
    fn new() -> Self {
        let mut lsg = LoopStructureGraph {
            root: 0,
            loops: Vector::new(),
            loop_counter: 0,
            heap: vec![SimpleLoop::new(None, true)],
        };
        lsg.heap[lsg.root].set_nesting_level(0);
        lsg.heap[lsg.root].counter = lsg.loop_counter;
        lsg.loop_counter += 1;
        lsg.loops.append(lsg.root);
        lsg
    }

    fn create_new_loop(&mut self, bb: usize, is_reducible: bool) -> usize {
        let mut loop_ = SimpleLoop::new(Some(bb), is_reducible);
        loop_.counter = self.loop_counter;
        self.loop_counter += 1;
        self.heap.push(loop_);
        let l = self.heap.len() - 1;
        self.loops.append(l);
        l
    }

    fn set_parent(&mut self, l: usize, parent: usize) {
        self.heap[l].parent = Some(parent);
        self.heap[parent].children.add(l);
    }

    fn calculate_nesting_level(&mut self) {
        // link up all 1st level loops to artificial root node.
        for i in self.loops.first_idx..self.loops.storage.len() {
            let liter = *self.loops.get(i);
            if !self.heap[liter].is_root && self.heap[liter].parent.is_none() {
                self.set_parent(liter, self.root);
            }
        }
        // recursively traverse the tree and assign levels.
        self.calculate_nesting_level_rec(self.root, 0);
    }

    fn calculate_nesting_level_rec(&mut self, l: usize, depth: usize) {
        self.heap[l].depth_level = depth;
        let children = &self.heap[l].children.items;
        for i in children.first_idx..children.storage.len() {
            let liter = *self.heap[l].children.items.get(i);
            self.calculate_nesting_level_rec(liter, depth + 1);
            let level = self.heap[l]
                .nesting_level
                .max(1 + self.heap[liter].nesting_level);
            self.heap[l].set_nesting_level(level);
        }
    }

    fn num_loops(&self) -> usize {
        self.loops.size()
    }
}

// ---- UnionFindNode

struct UnionFindNode {
    parent: usize,
    bb: Option<usize>,
    loop_: Option<usize>,
    dfs_number: usize,
}

/// The find routine of node `me`, with path compression: the nodes on the path whose parent is
/// not a root are pointed at `me`'s parent.
fn find_set(nodes: &mut [UnionFindNode], me: usize) -> usize {
    let mut node_list = Vector::new();
    let mut node = me;
    while node != nodes[node].parent {
        let parent = nodes[node].parent;
        if parent != nodes[parent].parent {
            node_list.append(node);
        }
        node = parent;
    }
    for &iter in node_list.live() {
        nodes[iter].parent = nodes[me].parent;
    }
    node
}

// ---- HavlakLoopFinder

#[derive(Clone, Copy, PartialEq)]
enum BbClass {
    Top,
    NonHeader,
    Reducible,
    SelfLoop,
    Irreducible,
    Dead,
}

struct HavlakLoopFinder<'a> {
    cfg: &'a ControlFlowGraph,
    lsg: &'a mut LoopStructureGraph,
    non_back_preds: Vector<Set<usize>>,
    back_preds: Vector<Vector<usize>>,
    number: IdentityDictionary,
    max_size: usize,
    header: Vec<usize>,
    typ: Vec<BbClass>,
    last: Vec<usize>,
    nodes: Vec<UnionFindNode>,
}

fn new_node() -> UnionFindNode {
    UnionFindNode {
        parent: 0,
        bb: None,
        loop_: None,
        dfs_number: 0,
    }
}

impl<'a> HavlakLoopFinder<'a> {
    fn new(cfg: &'a ControlFlowGraph, lsg: &'a mut LoopStructureGraph) -> Self {
        HavlakLoopFinder {
            cfg,
            lsg,
            non_back_preds: Vector::new(),
            back_preds: Vector::new(),
            number: IdentityDictionary::new(),
            max_size: 0,
            header: Vec::new(),
            typ: Vec::new(),
            last: Vec::new(),
            nodes: Vec::new(),
        }
    }

    fn is_ancestor(&self, w: usize, v: usize) -> bool {
        w <= v && v <= self.last[w]
    }

    fn do_dfs(&mut self, current_node: usize, current: usize) -> usize {
        let node = &mut self.nodes[current];
        node.parent = current;
        node.bb = Some(current_node);
        node.dfs_number = current;
        node.loop_ = None;
        self.number
            .at_put(current_node, self.cfg.custom_hash(current_node), current);
        let mut last_id = current;
        let outer_blocks = &self.cfg.blocks[current_node].out_edges;
        for i in 0..outer_blocks.size() {
            let target = *outer_blocks.get(i);
            if self.number.at(target, self.cfg.custom_hash(target)) == Some(UNVISITED) {
                last_id = self.do_dfs(target, last_id + 1);
            }
        }
        self.last[current] = last_id;
        last_id
    }

    fn init_all_nodes(&mut self) {
        for &bb in self.cfg.basic_block_map.live() {
            let bb = bb.expect("block");
            self.number.at_put(bb, self.cfg.custom_hash(bb), UNVISITED);
        }
        self.do_dfs(self.cfg.start_node.expect("start node"), 0);
    }

    fn identify_edges(&mut self, size: usize) {
        for w in 0..size {
            self.header[w] = 0;
            self.typ[w] = BbClass::NonHeader;
            match self.nodes[w].bb {
                None => self.typ[w] = BbClass::Dead,
                Some(node_w) => self.process_edges(node_w, w),
            }
        }
    }

    fn process_edges(&mut self, node_w: usize, w: usize) {
        let bb = &self.cfg.blocks[node_w];
        if bb.num_pred() > 0 {
            for &node_v in bb.in_edges.live() {
                let v = self
                    .number
                    .at(node_v, self.cfg.custom_hash(node_v))
                    .expect("numbered");
                if v != UNVISITED {
                    if self.is_ancestor(w, v) {
                        self.back_preds.get_mut(w).append(v);
                    } else {
                        self.non_back_preds.get_mut(w).add(v);
                    }
                }
            }
        }
    }

    fn find_loops(&mut self) {
        if self.cfg.start_node.is_none() {
            return;
        }
        let size = self.cfg.num_nodes();
        self.non_back_preds.remove_all();
        self.back_preds.remove_all();
        self.number.remove_all();
        if size > self.max_size {
            self.header = vec![0; size];
            self.typ = vec![BbClass::Top; size];
            self.last = vec![0; size];
            self.nodes = Vec::with_capacity(size);
            self.max_size = size;
        }
        for _ in 0..size {
            self.non_back_preds.append(Set::new());
            self.back_preds.append(Vector::new());
            self.nodes.push(new_node());
        }
        self.init_all_nodes();
        self.identify_edges(size);
        // Start node is root of all other loops.
        self.header[0] = 0;

        // Step c: reverse DFS preorder, so inner loop headers come before the loops around them.
        for w in (0..size).rev() {
            let mut node_pool = Vector::new(); // 'P' in Havlak's paper
            if let Some(node_w) = self.nodes[w].bb {
                self.step_d(w, &mut node_pool);
                // Copy nodePool to workList.
                let mut work_list = Vector::new();
                for &niter in node_pool.live() {
                    work_list.append(niter);
                }
                if node_pool.size() != 0 {
                    self.typ[w] = BbClass::Reducible;
                }
                // work the list...
                while !work_list.is_empty() {
                    let x = work_list.remove_first();
                    // Step e: the algorithm has degenerated past the safeguard: give up.
                    let non_back_size = self.non_back_preds.get(self.nodes[x].dfs_number).size();
                    if non_back_size > MAXNONBACKPREDS {
                        return;
                    }
                    self.step_e_process_non_back_preds(w, &mut node_pool, &mut work_list, x);
                }
                // Collapse the nodes of each SCC found into one, with a loop descriptor linked in.
                if node_pool.size() > 0 || self.typ[w] == BbClass::SelfLoop {
                    let l = self
                        .lsg
                        .create_new_loop(node_w, self.typ[w] != BbClass::Irreducible);
                    self.set_loop_attributes(w, &node_pool, l);
                }
            }
        }
    }

    fn step_e_process_non_back_preds(
        &mut self,
        w: usize,
        node_pool: &mut Vector<usize>,
        work_list: &mut Vector<usize>,
        x: usize,
    ) {
        let xs = self.nodes[x].dfs_number;
        let items = &self.non_back_preds.get(xs).items;
        for i in items.first_idx..items.storage.len() {
            let iter = *self.non_back_preds.get(xs).items.get(i);
            let ydash = find_set(&mut self.nodes, iter);
            let ydash_dfs = self.nodes[ydash].dfs_number;
            if !self.is_ancestor(w, ydash_dfs) {
                self.typ[w] = BbClass::Irreducible;
                self.non_back_preds.get_mut(w).add(ydash_dfs);
            } else if ydash_dfs != w && !node_pool.has_some(|&e| e == ydash) {
                work_list.append(ydash);
                node_pool.append(ydash);
            }
        }
    }

    fn set_loop_attributes(&mut self, w: usize, node_pool: &Vector<usize>, l: usize) {
        self.nodes[w].loop_ = Some(l);
        for &node in node_pool.live() {
            // Add nodes to loop descriptor.
            self.header[self.nodes[node].dfs_number] = w;
            self.nodes[node].parent = w;
            // Nested loops are not added, but linked together.
            match self.nodes[node].loop_ {
                Some(inner) => self.lsg.set_parent(inner, l),
                None => {
                    let bb = self.nodes[node].bb.expect("block");
                    self.lsg.heap[l].basic_blocks.add(bb);
                }
            }
        }
    }

    fn step_d(&mut self, w: usize, node_pool: &mut Vector<usize>) {
        for i in self.back_preds.get(w).first_idx..self.back_preds.get(w).storage.len() {
            let v = *self.back_preds.get(w).get(i);
            if v != w {
                node_pool.append(find_set(&mut self.nodes, v));
            } else {
                self.typ[w] = BbClass::SelfLoop;
            }
        }
    }
}

// ---- LoopTesterApp

struct LoopTesterApp {
    cfg: ControlFlowGraph,
    lsg: LoopStructureGraph,
}

impl LoopTesterApp {
    fn new() -> Self {
        let mut app = LoopTesterApp {
            cfg: ControlFlowGraph::new(),
            lsg: LoopStructureGraph::new(),
        };
        app.cfg.create_node(0);
        app
    }

    fn build_diamond(&mut self, start: usize) -> usize {
        let bb0 = start;
        self.cfg.basic_block_edge(bb0, bb0 + 1);
        self.cfg.basic_block_edge(bb0, bb0 + 2);
        self.cfg.basic_block_edge(bb0 + 1, bb0 + 3);
        self.cfg.basic_block_edge(bb0 + 2, bb0 + 3);
        bb0 + 3
    }

    fn build_connect(&mut self, start: usize, end: usize) {
        self.cfg.basic_block_edge(start, end);
    }

    fn build_straight(&mut self, start: usize, n: usize) -> usize {
        for i in 0..n {
            self.build_connect(start + i, start + i + 1);
        }
        start + n
    }

    fn build_base_loop(&mut self, from: usize) -> usize {
        let header = self.build_straight(from, 1);
        let diamond1 = self.build_diamond(header);
        let d11 = self.build_straight(diamond1, 1);
        let diamond2 = self.build_diamond(d11);
        let mut footer = self.build_straight(diamond2, 1);
        self.build_connect(diamond2, d11);
        self.build_connect(diamond1, header);
        self.build_connect(footer, from);
        footer = self.build_straight(footer, 1);
        footer
    }

    fn main(
        &mut self,
        num_dummy_loops: usize,
        find_loop_iterations: usize,
        par_loops: usize,
        ppar_loops: usize,
        pppar_loops: usize,
    ) -> [usize; 2] {
        self.construct_simple_cfg();
        self.add_dummy_loops(num_dummy_loops);
        self.construct_cfg(par_loops, ppar_loops, pppar_loops);
        // Performing Loop Recognition, 1 Iteration, then findLoopIteration
        HavlakLoopFinder::new(&self.cfg, &mut self.lsg).find_loops();
        for _ in 0..find_loop_iterations {
            let mut lsg = LoopStructureGraph::new();
            HavlakLoopFinder::new(&self.cfg, &mut lsg).find_loops();
        }
        self.lsg.calculate_nesting_level();
        [self.lsg.num_loops(), self.cfg.num_nodes()]
    }

    fn construct_cfg(&mut self, par_loops: usize, ppar_loops: usize, pppar_loops: usize) {
        let mut n = 2;
        for _ in 0..par_loops {
            self.cfg.create_node(n + 1);
            self.build_connect(2, n + 1);
            n += 1;
            for _ in 0..ppar_loops {
                let top = n;
                n = self.build_straight(n, 1);
                for _ in 0..pppar_loops {
                    n = self.build_base_loop(n);
                }
                let bottom = self.build_straight(n, 1);
                self.build_connect(n, top);
                n = bottom;
            }
            self.build_connect(n, 1);
        }
    }

    fn add_dummy_loops(&mut self, num_dummy_loops: usize) {
        for _ in 0..num_dummy_loops {
            HavlakLoopFinder::new(&self.cfg, &mut self.lsg).find_loops();
        }
    }

    fn construct_simple_cfg(&mut self) {
        self.cfg.create_node(0);
        self.build_base_loop(0);
        self.cfg.create_node(1);
        self.cfg.basic_block_edge(0, 2);
    }
}

fn run(n: i64) -> i64 {
    let n = usize::try_from(n).expect("non-negative size");
    let [loops, nodes] = LoopTesterApp::new().main(n, 50, 10, 10, 5);
    loops as i64 * 1_000_000 + nodes as i64
}

fn main() {
    sakura_bench_ports::main(run, 15000);
}
