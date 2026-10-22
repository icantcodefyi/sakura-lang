//! The region measurement of design §17.174 (2), bench-only behind the `regions` feature and
//! never in `stats()`: the bytes of `new_object` whose object is released before the direct twin
//! activation that allocated it returns. Native code built with `SAKURA_NATIVE_REGIONS=1` calls
//! [`Regions::enter`] at each twin entry and [`Regions::leave`] at each of its exits.

use std::collections::HashMap;

#[derive(Default)]
pub struct Regions {
    next: u64,
    /// The serials of the twin activations on the stack, innermost last (so ascending).
    live: Vec<u64>,
    /// Each counted object's allocating activation and size.
    born: HashMap<u32, (u64, u32)>,
    /// Bytes and objects of every `new_object`, and of those released inside their allocating
    /// twin (objects compare with `reuses`, which counts objects).
    pub requested: u64,
    pub inside: u64,
    pub objects: u64,
    pub objects_inside: u64,
}

impl Regions {
    pub fn enter(&mut self) {
        self.next += 1;
        self.live.push(self.next);
    }

    pub fn leave(&mut self) {
        self.live.pop();
    }

    pub fn alloc(&mut self, p: u32, size: u32) {
        self.requested += u64::from(size);
        self.objects += 1;
        match self.live.last() {
            Some(&s) => self.born.insert(p, (s, size)),
            None => self.born.remove(&p),
        };
    }

    pub fn free(&mut self, p: u32) {
        if let Some((s, size)) = self.born.remove(&p)
            && self.live.binary_search(&s).is_ok()
        {
            self.inside += u64::from(size);
            self.objects_inside += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Regions;

    #[test]
    fn counts_only_objects_released_before_their_twin_returns() {
        let mut r = Regions::default();
        r.enter();
        r.alloc(8, 16); // dies inside
        r.alloc(24, 32); // escapes
        r.free(8);
        r.leave();
        r.free(24);
        r.alloc(56, 8); // outside any twin
        r.free(56);
        r.enter();
        r.alloc(64, 8);
        r.enter(); // a callee frees its caller's object: the caller is still live
        r.free(64);
        r.leave();
        r.leave();
        assert_eq!((r.requested, r.inside), (64, 24));
        assert_eq!((r.objects, r.objects_inside), (4, 2));
    }
}
