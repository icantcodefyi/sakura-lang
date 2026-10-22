// CD from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/cd.js and
// C++/src/cd.h; Copyright (c) 2001-2010, Purdue University; Copyright (C) 2015 Apple Inc.; see
// ../LICENSE). AWFY's RedBlackTree keeps its parent pointers and in-place rotations, with the
// nodes in a per-tree arena and indices for pointers (NIL for null), as the DeltaBlue port does;
// keys compare through a `Key` trait (compareTo). SOM's Vector is a Vec; sin and cos are libm's.
// run(n) is cd(n - n % 2): AWFY's Simulator reads aircraft i + 1 for every even i, so it is
// defined for even n only.

const NIL: usize = usize::MAX;

#[derive(Clone, Copy)]
struct Vector2D {
    x: f64,
    y: f64,
}

impl Vector2D {
    fn plus(self, other: Vector2D) -> Vector2D {
        Vector2D {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    fn minus(self, other: Vector2D) -> Vector2D {
        Vector2D {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

fn compare_numbers(a: f64, b: f64) -> i32 {
    if a == b {
        return 0;
    }
    if a < b {
        return -1;
    }
    if a > b {
        return 1;
    }
    // We say that NaN is smaller than non-NaN.
    if a == a { 1 } else { -1 }
}

#[derive(Clone, Copy)]
struct Vector3D {
    x: f64,
    y: f64,
    z: f64,
}

impl Vector3D {
    fn plus(self, other: Vector3D) -> Vector3D {
        Vector3D {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }

    fn minus(self, other: Vector3D) -> Vector3D {
        Vector3D {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }

    fn dot(self, other: Vector3D) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn squared_magnitude(self) -> f64 {
        self.dot(self)
    }

    fn magnitude(self) -> f64 {
        self.squared_magnitude().sqrt()
    }

    fn times(self, amount: f64) -> Vector3D {
        Vector3D {
            x: self.x * amount,
            y: self.y * amount,
            z: self.z * amount,
        }
    }
}

#[derive(Clone, Copy)]
struct CallSign(i64);

trait Key: Copy {
    fn compare_to(&self, other: &Self) -> i32;
}

impl Key for Vector2D {
    fn compare_to(&self, other: &Self) -> i32 {
        let result = compare_numbers(self.x, other.x);
        if result != 0 {
            return result;
        }
        compare_numbers(self.y, other.y)
    }
}

impl Key for CallSign {
    fn compare_to(&self, other: &Self) -> i32 {
        if self.0 == other.0 {
            0
        } else if self.0 < other.0 {
            -1
        } else {
            1
        }
    }
}

// ---- red-black tree

struct Node<K, V> {
    key: K,
    value: V,
    left: usize,
    right: usize,
    parent: usize,
    red: bool,
}

struct RedBlackTree<K, V> {
    nodes: Vec<Node<K, V>>,
    root: usize,
}

impl<K: Key, V> RedBlackTree<K, V> {
    fn new() -> Self {
        RedBlackTree {
            nodes: Vec::new(),
            root: NIL,
        }
    }

    fn tree_minimum(&self, x: usize) -> usize {
        let mut current = x;
        while self.nodes[current].left != NIL {
            current = self.nodes[current].left;
        }
        current
    }

    fn successor(&self, node: usize) -> usize {
        let mut x = node;
        if self.nodes[x].right != NIL {
            return self.tree_minimum(self.nodes[x].right);
        }
        let mut y = self.nodes[x].parent;
        while y != NIL && x == self.nodes[y].right {
            x = y;
            y = self.nodes[y].parent;
        }
        y
    }

    fn is_red(&self, n: usize) -> bool {
        n != NIL && self.nodes[n].red
    }

    /// AWFY's put: the replaced value, or None after inserting a new entry.
    fn put(&mut self, key: K, value: V) -> Option<V> {
        let mut x = match self.tree_insert(key, value) {
            Err(old) => return Some(old),
            Ok(z) => z,
        };
        while x != self.root && self.nodes[self.nodes[x].parent].red {
            let p = self.nodes[x].parent;
            let g = self.nodes[p].parent;
            if p == self.nodes[g].left {
                let y = self.nodes[g].right;
                if self.is_red(y) {
                    // Case 1
                    self.nodes[p].red = false;
                    self.nodes[y].red = false;
                    self.nodes[g].red = true;
                    x = g;
                } else {
                    if x == self.nodes[p].right {
                        // Case 2
                        x = p;
                        self.left_rotate(x);
                    }
                    // Case 3
                    let p = self.nodes[x].parent;
                    let g = self.nodes[p].parent;
                    self.nodes[p].red = false;
                    self.nodes[g].red = true;
                    self.right_rotate(g);
                }
            } else {
                // Same as "then" clause with "right" and "left" exchanged.
                let y = self.nodes[g].left;
                if self.is_red(y) {
                    // Case 1
                    self.nodes[p].red = false;
                    self.nodes[y].red = false;
                    self.nodes[g].red = true;
                    x = g;
                } else {
                    if x == self.nodes[p].left {
                        // Case 2
                        x = p;
                        self.right_rotate(x);
                    }
                    // Case 3
                    let p = self.nodes[x].parent;
                    let g = self.nodes[p].parent;
                    self.nodes[p].red = false;
                    self.nodes[g].red = true;
                    self.left_rotate(g);
                }
            }
        }
        let root = self.root;
        self.nodes[root].red = false;
        None
    }

    /// AWFY's remove: the removed value, if the key was present. The unlinked node stays in the
    /// arena (CD never removes: every aircraft is in every frame).
    fn remove(&mut self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        let z = self.find_node(key)?;

        // Y is the node to be unlinked from the tree.
        let y = if self.nodes[z].left == NIL || self.nodes[z].right == NIL {
            z
        } else {
            self.successor(z)
        };

        // X is the child of y which might potentially replace y in the tree. X might be NIL.
        let x = if self.nodes[y].left != NIL {
            self.nodes[y].left
        } else {
            self.nodes[y].right
        };
        let y_parent = self.nodes[y].parent;
        if x != NIL {
            self.nodes[x].parent = y_parent;
        }
        let x_parent = y_parent;
        if y_parent == NIL {
            self.root = x;
        } else if y == self.nodes[y_parent].left {
            self.nodes[y_parent].left = x;
        } else {
            self.nodes[y_parent].right = x;
        }

        if y != z {
            if !self.nodes[y].red {
                self.remove_fixup(x, x_parent);
            }
            let (z_parent, z_red, z_left, z_right) = (
                self.nodes[z].parent,
                self.nodes[z].red,
                self.nodes[z].left,
                self.nodes[z].right,
            );
            self.nodes[y].parent = z_parent;
            self.nodes[y].red = z_red;
            self.nodes[y].left = z_left;
            self.nodes[y].right = z_right;
            if z_left != NIL {
                self.nodes[z_left].parent = y;
            }
            if z_right != NIL {
                self.nodes[z_right].parent = y;
            }
            if z_parent != NIL {
                if self.nodes[z_parent].left == z {
                    self.nodes[z_parent].left = y;
                } else {
                    self.nodes[z_parent].right = y;
                }
            } else {
                self.root = y;
            }
        } else if !self.nodes[y].red {
            self.remove_fixup(x, x_parent);
        }
        Some(self.nodes[z].value.clone())
    }

    fn get(&self, key: &K) -> Option<&V> {
        self.find_node(key).map(|n| &self.nodes[n].value)
    }

    fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.find_node(key).map(|n| &mut self.nodes[n].value)
    }

    /// AWFY's forEach: every entry in key order.
    fn for_each(&mut self, mut f: impl FnMut(&K, &mut V)) {
        if self.root == NIL {
            return;
        }
        let mut current = self.tree_minimum(self.root);
        while current != NIL {
            let node = &mut self.nodes[current];
            f(&node.key, &mut node.value);
            current = self.successor(current);
        }
    }

    fn find_node(&self, key: &K) -> Option<usize> {
        let mut current = self.root;
        while current != NIL {
            let comparison = key.compare_to(&self.nodes[current].key);
            if comparison == 0 {
                return Some(current);
            }
            current = if comparison < 0 {
                self.nodes[current].left
            } else {
                self.nodes[current].right
            };
        }
        None
    }

    /// The new node, or the old value after replacing an existing entry's value.
    fn tree_insert(&mut self, key: K, value: V) -> Result<usize, V> {
        let mut y = NIL;
        let mut x = self.root;
        while x != NIL {
            y = x;
            let comparison = key.compare_to(&self.nodes[x].key);
            if comparison < 0 {
                x = self.nodes[x].left;
            } else if comparison > 0 {
                x = self.nodes[x].right;
            } else {
                return Err(std::mem::replace(&mut self.nodes[x].value, value));
            }
        }

        let z = self.nodes.len();
        self.nodes.push(Node {
            key,
            value,
            left: NIL,
            right: NIL,
            parent: y,
            red: true,
        });
        if y == NIL {
            self.root = z;
        } else if key.compare_to(&self.nodes[y].key) < 0 {
            self.nodes[y].left = z;
        } else {
            self.nodes[y].right = z;
        }
        Ok(z)
    }

    fn left_rotate(&mut self, x: usize) {
        let y = self.nodes[x].right;

        // Turn y's left subtree into x's right subtree.
        let y_left = self.nodes[y].left;
        self.nodes[x].right = y_left;
        if y_left != NIL {
            self.nodes[y_left].parent = x;
        }

        // Link x's parent to y.
        let x_parent = self.nodes[x].parent;
        self.nodes[y].parent = x_parent;
        if x_parent == NIL {
            self.root = y;
        } else if x == self.nodes[x_parent].left {
            self.nodes[x_parent].left = y;
        } else {
            self.nodes[x_parent].right = y;
        }

        // Put x on y's left.
        self.nodes[y].left = x;
        self.nodes[x].parent = y;
    }

    fn right_rotate(&mut self, y: usize) {
        let x = self.nodes[y].left;

        // Turn x's right subtree into y's left subtree.
        let x_right = self.nodes[x].right;
        self.nodes[y].left = x_right;
        if x_right != NIL {
            self.nodes[x_right].parent = y;
        }

        // Link y's parent to x.
        let y_parent = self.nodes[y].parent;
        self.nodes[x].parent = y_parent;
        if y_parent == NIL {
            self.root = x;
        } else if y == self.nodes[y_parent].left {
            self.nodes[y_parent].left = x;
        } else {
            self.nodes[y_parent].right = x;
        }

        self.nodes[x].right = y;
        self.nodes[y].parent = x;
    }

    fn remove_fixup(&mut self, mut x: usize, mut x_parent: usize) {
        while x != self.root && !self.is_red(x) {
            if x == self.nodes[x_parent].left {
                // The text points out that w cannot be NIL: it follows from the red-black
                // properties.
                let mut w = self.nodes[x_parent].right;
                if self.nodes[w].red {
                    // Case 1
                    self.nodes[w].red = false;
                    self.nodes[x_parent].red = true;
                    self.left_rotate(x_parent);
                    w = self.nodes[x_parent].right;
                }
                if !self.is_red(self.nodes[w].left) && !self.is_red(self.nodes[w].right) {
                    // Case 2
                    self.nodes[w].red = true;
                    x = x_parent;
                    x_parent = self.nodes[x].parent;
                } else {
                    if !self.is_red(self.nodes[w].right) {
                        // Case 3
                        let w_left = self.nodes[w].left;
                        self.nodes[w_left].red = false;
                        self.nodes[w].red = true;
                        self.right_rotate(w);
                        w = self.nodes[x_parent].right;
                    }
                    // Case 4
                    self.nodes[w].red = self.nodes[x_parent].red;
                    self.nodes[x_parent].red = false;
                    let w_right = self.nodes[w].right;
                    if w_right != NIL {
                        self.nodes[w_right].red = false;
                    }
                    self.left_rotate(x_parent);
                    x = self.root;
                    x_parent = self.nodes[x].parent;
                }
            } else {
                // Same as "then" clause with "right" and "left" exchanged.
                let mut w = self.nodes[x_parent].left;
                if self.nodes[w].red {
                    // Case 1
                    self.nodes[w].red = false;
                    self.nodes[x_parent].red = true;
                    self.right_rotate(x_parent);
                    w = self.nodes[x_parent].left;
                }
                if !self.is_red(self.nodes[w].right) && !self.is_red(self.nodes[w].left) {
                    // Case 2
                    self.nodes[w].red = true;
                    x = x_parent;
                    x_parent = self.nodes[x].parent;
                } else {
                    if !self.is_red(self.nodes[w].left) {
                        // Case 3
                        let w_right = self.nodes[w].right;
                        self.nodes[w_right].red = false;
                        self.nodes[w].red = true;
                        self.left_rotate(w);
                        w = self.nodes[x_parent].left;
                    }
                    // Case 4
                    self.nodes[w].red = self.nodes[x_parent].red;
                    self.nodes[x_parent].red = false;
                    let w_left = self.nodes[w].left;
                    if w_left != NIL {
                        self.nodes[w_left].red = false;
                    }
                    self.right_rotate(x_parent);
                    x = self.root;
                    x_parent = self.nodes[x].parent;
                }
            }
        }
        if x != NIL {
            self.nodes[x].red = false;
        }
    }
}

// ---- the simulation

const MIN_X: f64 = 0.0;
const MIN_Y: f64 = 0.0;
const MAX_X: f64 = 1000.0;
const MAX_Y: f64 = 1000.0;
const MIN_Z: f64 = 0.0;
const MAX_Z: f64 = 10.0;
const PROXIMITY_RADIUS: f64 = 1.0;
const GOOD_VOXEL_SIZE: f64 = PROXIMITY_RADIUS * 2.0;

const HORIZONTAL: Vector2D = Vector2D {
    x: GOOD_VOXEL_SIZE,
    y: 0.0,
};
const VERTICAL: Vector2D = Vector2D {
    x: 0.0,
    y: GOOD_VOXEL_SIZE,
};

#[derive(Clone, Copy)]
struct Motion {
    callsign: CallSign,
    pos_one: Vector3D,
    pos_two: Vector3D,
}

#[allow(dead_code)]
struct Collision {
    aircraft_a: CallSign,
    aircraft_b: CallSign,
    position: Vector3D,
}

struct Aircraft {
    callsign: CallSign,
    position: Vector3D,
}

impl Motion {
    fn delta(&self) -> Vector3D {
        self.pos_two.minus(self.pos_one)
    }

    fn find_intersection(&self, other: &Motion) -> Option<Vector3D> {
        let init1 = self.pos_one;
        let init2 = other.pos_one;
        let vec1 = self.delta();
        let vec2 = other.delta();
        let radius = PROXIMITY_RADIUS;

        // a = (V2 - V1)^T * (V2 - V1)
        let a = vec2.minus(vec1).squared_magnitude();

        if a != 0.0 {
            // b = 2 * <I1-I2, V1-V2>
            let b = 2.0 * init1.minus(init2).dot(vec1.minus(vec2));
            // c = -r^2 + (I2 - I1)^T * (I2 - I1)
            let c = -radius * radius + init2.minus(init1).squared_magnitude();

            let discr = b * b - 4.0 * a * c;
            if discr < 0.0 {
                return None;
            }

            let v1 = (-b - discr.sqrt()) / (2.0 * a);
            let v2 = (-b + discr.sqrt()) / (2.0 * a);

            if v1 <= v2
                && ((v1 <= 1.0 && 1.0 <= v2)
                    || (v1 <= 0.0 && 0.0 <= v2)
                    || (0.0 <= v1 && v2 <= 1.0))
            {
                // Pick a good "time" at which to report the collision.
                let v = if v1 <= 0.0 { 0.0 } else { v1 };

                let result1 = init1.plus(vec1.times(v));
                let result2 = init2.plus(vec2.times(v));

                let result = result1.plus(result2).times(0.5);
                if result.x >= MIN_X
                    && result.x <= MAX_X
                    && result.y >= MIN_Y
                    && result.y <= MAX_Y
                    && result.z >= MIN_Z
                    && result.z <= MAX_Z
                {
                    return Some(result);
                }
            }
            return None;
        }

        // The planes have the same speeds and are moving in parallel (or not moving at all).
        let dist = init2.minus(init1).magnitude();
        if dist <= radius {
            return Some(init1.plus(init2).times(0.5));
        }
        None
    }
}

fn is_in_voxel(voxel: Vector2D, motion: &Motion) -> bool {
    if voxel.x > MAX_X || voxel.x < MIN_X || voxel.y > MAX_Y || voxel.y < MIN_Y {
        return false;
    }

    let init = motion.pos_one;
    let fin = motion.pos_two;

    let v_s = GOOD_VOXEL_SIZE;
    let r = PROXIMITY_RADIUS / 2.0;

    let v_x = voxel.x;
    let x0 = init.x;
    let xv = fin.x - init.x;

    let v_y = voxel.y;
    let y0 = init.y;
    let yv = fin.y - init.y;

    let mut low_x = (v_x - r - x0) / xv;
    let mut high_x = (v_x + v_s + r - x0) / xv;

    if xv < 0.0 {
        std::mem::swap(&mut low_x, &mut high_x);
    }

    let mut low_y = (v_y - r - y0) / yv;
    let mut high_y = (v_y + v_s + r - y0) / yv;

    if yv < 0.0 {
        std::mem::swap(&mut low_y, &mut high_y);
    }

    // The first disjunct of each of the three conjuncts is "no motion in x", "no motion in y" and
    // "no motion in x or y or both".
    ((xv == 0.0 && v_x <= x0 + r && x0 - r <= v_x + v_s)
        || ((low_x <= 1.0 && 1.0 <= high_x)
            || (low_x <= 0.0 && 0.0 <= high_x)
            || (0.0 <= low_x && high_x <= 1.0)))
        && ((yv == 0.0 && v_y <= y0 + r && y0 - r <= v_y + v_s)
            || ((low_y <= 1.0 && 1.0 <= high_y)
                || (low_y <= 0.0 && 0.0 <= high_y)
                || (0.0 <= low_y && high_y <= 1.0)))
        && (xv == 0.0
            || yv == 0.0
            || (low_y <= high_x && high_x <= high_y)
            || (low_y <= low_x && low_x <= high_y)
            || (low_x <= low_y && high_y <= high_x))
}

type VoxelMap = RedBlackTree<Vector2D, Vec<Motion>>;

fn put_into_map(voxel_map: &mut VoxelMap, voxel: Vector2D, motion: &Motion) {
    match voxel_map.get_mut(&voxel) {
        Some(vec) => vec.push(*motion),
        None => {
            voxel_map.put(voxel, vec![*motion]);
        }
    }
}

fn recurse(
    voxel_map: &mut VoxelMap,
    seen: &mut RedBlackTree<Vector2D, bool>,
    next_voxel: Vector2D,
    motion: &Motion,
) {
    if !is_in_voxel(next_voxel, motion) {
        return;
    }
    if seen.put(next_voxel, true) == Some(true) {
        return;
    }

    put_into_map(voxel_map, next_voxel, motion);

    recurse(voxel_map, seen, next_voxel.minus(HORIZONTAL), motion);
    recurse(voxel_map, seen, next_voxel.plus(HORIZONTAL), motion);
    recurse(voxel_map, seen, next_voxel.minus(VERTICAL), motion);
    recurse(voxel_map, seen, next_voxel.plus(VERTICAL), motion);
    recurse(
        voxel_map,
        seen,
        next_voxel.minus(HORIZONTAL).minus(VERTICAL),
        motion,
    );
    recurse(
        voxel_map,
        seen,
        next_voxel.minus(HORIZONTAL).plus(VERTICAL),
        motion,
    );
    recurse(
        voxel_map,
        seen,
        next_voxel.plus(HORIZONTAL).minus(VERTICAL),
        motion,
    );
    recurse(
        voxel_map,
        seen,
        next_voxel.plus(HORIZONTAL).plus(VERTICAL),
        motion,
    );
}

fn voxel_hash(position: Vector3D) -> Vector2D {
    let x_div = (position.x / GOOD_VOXEL_SIZE) as i32;
    let y_div = (position.y / GOOD_VOXEL_SIZE) as i32;

    let mut result = Vector2D {
        x: GOOD_VOXEL_SIZE * x_div as f64,
        y: GOOD_VOXEL_SIZE * y_div as f64,
    };

    if position.x < 0.0 {
        result.x -= GOOD_VOXEL_SIZE;
    }
    if position.y < 0.0 {
        result.y -= GOOD_VOXEL_SIZE;
    }
    result
}

fn draw_motion_on_voxel_map(voxel_map: &mut VoxelMap, motion: &Motion) {
    let mut seen = RedBlackTree::new();
    recurse(voxel_map, &mut seen, voxel_hash(motion.pos_one), motion);
}

fn reduce_collision_set(motions: &[Motion]) -> Vec<Vec<Motion>> {
    let mut voxel_map = VoxelMap::new();
    for motion in motions {
        draw_motion_on_voxel_map(&mut voxel_map, motion);
    }

    let mut result = Vec::new();
    voxel_map.for_each(|_, vec| {
        if vec.len() > 1 {
            result.push(std::mem::take(vec));
        }
    });
    result
}

struct CollisionDetector {
    state: RedBlackTree<CallSign, Vector3D>,
}

impl CollisionDetector {
    fn handle_new_frame(&mut self, frame: &[Aircraft]) -> Vec<Collision> {
        let mut motions = Vec::new();
        let mut seen = RedBlackTree::new();

        for aircraft in frame {
            let old_position = self.state.put(aircraft.callsign, aircraft.position);
            let new_position = aircraft.position;
            seen.put(aircraft.callsign, true);

            // Treat newly introduced aircraft as if they were stationary.
            let old_position = old_position.unwrap_or(new_position);
            motions.push(Motion {
                callsign: aircraft.callsign,
                pos_one: old_position,
                pos_two: new_position,
            });
        }

        // Remove aircraft that are no longer present.
        let mut to_remove = Vec::new();
        self.state.for_each(|callsign, _| {
            if seen.get(callsign) != Some(&true) {
                to_remove.push(*callsign);
            }
        });
        for callsign in &to_remove {
            self.state.remove(callsign);
        }

        let all_reduced = reduce_collision_set(&motions);
        let mut collisions = Vec::new();

        for reduced in &all_reduced {
            for i in 0..reduced.len() {
                let motion1 = &reduced[i];
                for motion2 in &reduced[i + 1..] {
                    if let Some(position) = motion1.find_intersection(motion2) {
                        collisions.push(Collision {
                            aircraft_a: motion1.callsign,
                            aircraft_b: motion2.callsign,
                            position,
                        });
                    }
                }
            }
        }
        collisions
    }
}

struct Simulator {
    aircraft: Vec<CallSign>,
}

impl Simulator {
    fn new(num_aircraft: i64) -> Self {
        Simulator {
            aircraft: (0..num_aircraft).map(CallSign).collect(),
        }
    }

    fn simulate(&self, time: f64) -> Vec<Aircraft> {
        let mut frame = Vec::new();
        for i in (0..self.aircraft.len()).step_by(2) {
            frame.push(Aircraft {
                callsign: self.aircraft[i],
                position: Vector3D {
                    x: time,
                    y: time.cos() * 2.0 + (i * 3) as f64,
                    z: 10.0,
                },
            });
            frame.push(Aircraft {
                callsign: self.aircraft[i + 1],
                position: Vector3D {
                    x: time,
                    y: time.sin() * 2.0 + (i * 3) as f64,
                    z: 10.0,
                },
            });
        }
        frame
    }
}

fn cd(num_aircrafts: i64) -> i64 {
    let num_frames = 200;
    let simulator = Simulator::new(num_aircrafts);
    let mut detector = CollisionDetector {
        state: RedBlackTree::new(),
    };

    let mut actual_collisions = 0;
    for i in 0..num_frames {
        let time = i as f64 / 10.0;
        let collisions = detector.handle_new_frame(&simulator.simulate(time));
        actual_collisions += collisions.len() as i64;
    }
    actual_collisions
}

fn run(n: i64) -> i64 {
    cd(n - n % 2)
}

fn main() {
    sakura_bench_ports::main(run, 1000);
}
