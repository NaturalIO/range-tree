#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]

//! This crate provides a range-tree implementation, intended to manage range section with btree.
//!
//! [RangeTreeCustom<T>] is a generic for slab allocators. While for other simple usage, use the alias
//! [RangeTree].

use core::{
    cmp::Ordering,
    fmt,
    ops::{AddAssign, SubAssign},
};
use embed_collections::btree::{BTreeMap, Entry};
use num_traits::*;

pub trait RangeTreeKey:
    Unsigned + AddAssign + SubAssign + Ord + Copy + fmt::Debug + fmt::Display + Default
{
}

impl<T> RangeTreeKey for T where
    T: Unsigned + AddAssign + SubAssign + Ord + Copy + fmt::Debug + fmt::Display + Default
{
}

pub struct RangeTreeCustom<T: RangeTreeKey, O>
where
    O: RangeTreeOps<T>,
{
    tree: BTreeMap<T, T>,
    space: T,
    ops: O,
}

/// A trait for allocator, triggers when range segment add /remove from the main RangeTree.
pub trait RangeTreeOps<T: RangeTreeKey>: Sized + Default {
    /// Callback for manage secondary tree
    fn op_add(&mut self, start: T, end: T);
    /// Callback for manage secondary tree
    fn op_remove(&mut self, start: T, end: T);
}

pub type RangeTree<T> = RangeTreeCustom<T, DummyAllocator>;

#[derive(Default)]
pub struct DummyAllocator();

impl<T: RangeTreeKey> RangeTreeOps<T> for DummyAllocator {
    #[inline]
    fn op_add(&mut self, _start: T, _end: T) {}

    #[inline]
    fn op_remove(&mut self, _start: T, _end: T) {}
}

/*
pub struct RangeTreeIter<'a, T: RangeTreeOps<T>> {
    tree: &'a RangeTree<T>,
    current: Option<&'a RangeSeg<T>>,
}

unsafe impl<'a, T: RangeTreeOps<T>> Send for RangeTreeIter<'a, T> {}

impl<'a, T: RangeTreeOps<T>> Iterator for RangeTreeIter<'a, T> {
    type Item = &'a RangeSeg<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current.take();
        if let Some(seg) = current {
            self.current = self.tree.root.next(seg);
        }
        current
    }
}

impl<'a, T: RangeTreeOps<T>> IntoIterator for &'a RangeTree<T> {
    type Item = &'a RangeSeg<T>;
    type IntoIter = RangeTreeIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[allow(dead_code)]
impl<T: RangeTreeOps<T>> Default for RangeTree<T> {
    fn default() -> Self {
        Self::new()
    }
}
*/

impl<T: RangeTreeKey, O: RangeTreeOps<T>> RangeTreeCustom<T, O> {
    pub fn new() -> Self {
        Self { tree: BTreeMap::new(), space: T::zero(), ops: O::default() }
    }

    #[inline]
    pub fn get_ops(&self) -> &O {
        &self.ops
    }

    #[inline]
    pub fn get_ops_mut(&mut self) -> &mut O {
        &mut self.ops
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    #[inline(always)]
    pub fn get_space(&self) -> T {
        self.space
    }

    #[inline(always)]
    pub fn get_count(&self) -> usize {
        self.tree.len()
    }

    /// Add range segment, merge with adjacent ranges.
    ///
    /// Returns `Ok(())` if there are no intersection;
    /// otherwise returns the overlapping range as `Err((existing_start, existing_size))`.
    #[inline]
    pub fn add(&mut self, start: T, size: T) -> Result<(), (T, T)> {
        assert!(size > T::zero(), "range tree add size={} error", size);
        let mut add_size = size;
        match self.tree.entry(start) {
            Entry::Occupied(ent) => {
                return Err((*ent.key(), *ent.get()));
            }
            Entry::Vacant(ent) => {
                let merge_before = if let Some((_start, _size)) = ent.peak_backward() {
                    let _end = *_start + *_size;
                    match _end.cmp(&start) {
                        Ordering::Equal => true,
                        Ordering::Greater => return Err((*_start, *_size)),
                        _ => false,
                    }
                } else {
                    false
                };
                let merge_after = if let Some((_start, _size)) = ent.peak_forward() {
                    match (start + size).cmp(_start) {
                        Ordering::Equal => {
                            if merge_before {
                                // avoid visiting the node again
                                add_size += *_size;
                            }
                            true
                        }
                        Ordering::Greater => return Err((*_start, *_size)),
                        _ => false,
                    }
                } else {
                    false
                };

                match (merge_before, merge_after) {
                    (false, false) => {
                        self.ops.op_add(start, start + size);
                        ent.insert(size);
                        self.space += size;
                    }
                    (false, true) => {
                        let mut ent_next = ent.move_forward().expect("merge next");
                        let next_start = *ent_next.key();
                        let next_size = *ent_next.get();

                        self.ops.op_remove(next_start, next_start + next_size);

                        *ent_next.get_mut() += size;
                        ent_next.alter_key(start).expect("merge next alter_key");
                        self.space += size;

                        self.ops.op_add(start, next_start + next_size);
                    }
                    (true, false) => {
                        let ent_prev_res = ent.move_backward();
                        let mut ent_prev = ent_prev_res.expect("merge prev");
                        let prev_start = *ent_prev.key();
                        let prev_size = *ent_prev.get();

                        self.ops.op_remove(prev_start, prev_start + prev_size);

                        *ent_prev.get_mut() += size;
                        self.space += size;

                        self.ops.op_add(prev_start, prev_start + prev_size + size);
                    }
                    (true, true) => {
                        let ent_prev = ent.move_backward().expect("merge prev");
                        let prev_start = *ent_prev.key();
                        let prev_size = *ent_prev.get();
                        let ent_next = ent_prev.move_forward().expect("merge next");
                        let next_start = *ent_next.key();
                        let next_size = *ent_next.get();

                        self.ops.op_remove(prev_start, prev_start + prev_size);
                        self.ops.op_remove(next_start, next_start + next_size);

                        // Refetch prev entry after moving forward and back or similar.
                        let mut ent_prev = ent_next.move_backward().expect("merge prev refetch");
                        *ent_prev.get_mut() += add_size;
                        self.space += size; // only the newly added size contributes to space increase
                        let ent_next = ent_prev.move_forward().expect("merge next");
                        ent_next.remove();

                        self.ops.op_add(prev_start, next_start + next_size);
                    }
                }
                Ok(())
            }
        }
    }

    #[inline(always)]
    pub fn add_abs(&mut self, start: T, end: T) {
        assert!(start < end, "range tree add start={} end={}", start, end);
        let _ = self.add(start, end - start);
    }

    /// Add range which may be crossed section or larger with existing, will merge the range
    #[inline]
    pub fn add_and_merge(&mut self, start: T, size: T) {
        assert!(size > T::zero(), "range tree add size error");
        let new_end = start + size;
        let mut handled_by_recursion = false;

        let base_ent = match self.tree.entry(start) {
            Entry::Occupied(oe) => {
                if start + *oe.get() >= new_end {
                    return;
                }
                Entry::Occupied(oe)
            }
            Entry::Vacant(ve) => {
                if let Some((pre_start, pre_size)) = ve.peak_backward() {
                    let cur_end = *pre_start + *pre_size;
                    if cur_end >= new_end {
                        return;
                    }
                    if cur_end >= start {
                        Entry::Occupied(ve.move_backward().expect("move back to merge"))
                    } else {
                        Entry::Vacant(ve)
                    }
                } else {
                    Entry::Vacant(ve)
                }
            }
        };

        macro_rules! remove_intersect {
            ($next_start: expr, $new_end: expr) => {
                if let Some((last_start, last_size)) = self.tree.remove_range_with(
                    $next_start..=$new_end,
                    |removed_start, removed_size| {
                        self.space -= *removed_size;
                        self.ops.op_remove(*removed_start, *removed_start + *removed_size);
                    },
                ) {
                    let last_end = last_start + last_size;
                    if last_end > new_end {
                        let _size = last_end - new_end;
                        // add back and join with previous range
                        self.add(new_end, _size)
                            .expect("add {new_end:?}:{_size:?} should not fail");
                        handled_by_recursion = true;
                    }
                }
            };
        }
        match base_ent {
            Entry::Occupied(mut oe) => {
                let base_start = *oe.key();
                let old_size = *oe.get();
                self.ops.op_remove(base_start, base_start + old_size);

                // extend the size to final size
                let final_size = new_end - base_start;
                self.space += final_size - old_size;
                *oe.get_mut() = final_size;

                if let Some((_next_start, _next_size)) = oe.peak_forward() {
                    let next_start = *_next_start;
                    let next_size = *_next_size;
                    if next_start < new_end {
                        drop(oe);
                        remove_intersect!(next_start, new_end);
                    } else if next_start == new_end {
                        self.ops.op_remove(next_start, next_start + next_size);
                        // space is neutral (moving between segments)
                        *oe.get_mut() += next_size;
                        self.tree.remove(&next_start);
                    }
                }

                if !handled_by_recursion {
                    let final_key = base_start;
                    let final_size = if let Entry::Occupied(o) = self.tree.entry(final_key) {
                        *o.get()
                    } else {
                        unreachable!()
                    };
                    self.ops.op_add(final_key, final_key + final_size);
                }
            }
            Entry::Vacant(ve) => {
                let base_start = start;
                self.space += size;

                if let Some((_next_start, _next_size)) = ve.peak_forward() {
                    let next_start = *_next_start;
                    let next_size = *_next_size;
                    if next_start < new_end {
                        ve.insert(size);
                        remove_intersect!(next_start, new_end);
                        if !handled_by_recursion {
                            if let Entry::Occupied(o) = self.tree.entry(base_start) {
                                self.ops.op_add(base_start, base_start + *o.get());
                            }
                        }
                    } else if next_start == new_end {
                        let final_size = new_end - base_start + next_size;
                        self.ops.op_remove(next_start, next_start + next_size);
                        ve.insert(final_size);
                        self.tree.remove(&next_start);
                        self.ops.op_add(base_start, base_start + final_size);
                    } else {
                        ve.insert(size);
                        self.ops.op_add(base_start, base_start + size);
                    }
                } else {
                    ve.insert(size);
                    self.ops.op_add(base_start, base_start + size);
                }
            }
        }
    }

    /// Ensure remove all overlapping range
    ///
    /// Returns true if removal happens
    #[inline(always)]
    pub fn remove_and_split(&mut self, start: T, size: T) -> bool {
        let mut removed = false;
        while self.remove(start, size) {
            removed = true;
        }
        removed
    }

    /// Only used when remove range overlap one segment,
    ///
    /// NOTE: If not the case (start, size) might overlaps with multiple segment,  use remove_and_split() instead.
    /// return true when one segment is removed.
    #[inline]
    pub fn remove(&mut self, start: T, size: T) -> bool {
        let end = start + size;
        match self.tree.entry(start) {
            Entry::Occupied(mut oent) => {
                let rs_size = *oent.get();
                self.ops.op_remove(start, start + rs_size);
                if rs_size > size {
                    // Shrink from front
                    let new_start = start + size;
                    let new_size = rs_size - size;
                    oent.alter_key(new_start).expect("shrink alter_key");
                    *oent.get_mut() = new_size;
                    self.ops.op_add(new_start, new_start + new_size);
                    self.space -= size;
                } else {
                    // Exact match or subset removed
                    oent.remove();
                    self.space -= rs_size;
                }
                true
            }
            Entry::Vacant(vent) => {
                if let Some((&rs_start, &rs_size)) = vent.peak_backward() {
                    let rs_end = rs_start + rs_size;
                    if rs_end > start {
                        let mut oent = vent.move_backward().expect("move back to overlapping");
                        self.ops.op_remove(rs_start, rs_end);
                        let size_deduce: T;

                        if rs_end > end {
                            // Split in middle
                            *oent.get_mut() = start - rs_start;
                            self.tree.insert(end, rs_end - end);
                            self.ops.op_add(rs_start, start);
                            self.ops.op_add(end, rs_end);
                            size_deduce = size;
                        } else {
                            // Shrink from back
                            *oent.get_mut() = start - rs_start;
                            self.ops.op_add(rs_start, start);
                            size_deduce = rs_end - start;
                        }
                        self.space -= size_deduce;
                        return true;
                    }
                }

                // Handle the case where range starts before the first overlapping segment
                if let Some((&ns, _)) = vent.peak_forward() {
                    if ns < end {
                        let mut oent = vent.move_forward().expect("move forward to overlapping");
                        let rs_start = *oent.key();
                        let rs_size = *oent.get();
                        let rs_end = rs_start + rs_size;

                        self.ops.op_remove(rs_start, rs_end);
                        if rs_end > end {
                            // Shrink from front
                            let new_start = end;
                            let new_size = rs_end - end;
                            oent.alter_key(new_start).expect("shrink forward alter_key");
                            *oent.get_mut() = new_size;
                            self.ops.op_add(new_start, rs_end);
                            self.space -= end - rs_start;
                        } else {
                            // Entirely removed
                            oent.remove();
                            self.space -= rs_size;
                        }
                        return true;
                    }
                }
                false
            }
        }
    }

    /// return only when segment overlaps with [start, start+size]
    #[inline]
    pub fn find(&self, start: T, size: T) -> Option<(T, T)> {
        if self.tree.is_empty() {
            return None;
        }
        let end = start + size;

        // 1. Check for a segment that starts before 'start' but might overlap it
        if let Some((&k, &sz)) = self.tree.range(..start).next_back() {
            if k + sz > start {
                return Some((k, sz));
            }
        }

        // 2. Check for the first segment that starts within [start, end)
        self.tree.range(start..end).next().map(|(&k, &sz)| (k, sz))
    }

    #[inline]
    pub fn iter(&self) -> embed_collections::btree::Iter<'_, T, T> {
        self.tree.iter()
    }

    #[inline]
    pub fn walk<F: FnMut((T, T))>(&self, mut cb: F) {
        for (&start, &size) in self.tree.iter() {
            cb((start, size));
        }
    }

    #[inline]
    pub fn walk_conditioned<F: FnMut((T, T)) -> bool>(&self, mut cb: F) {
        for (&start, &size) in self.tree.iter() {
            if !cb((start, size)) {
                break;
            }
        }
    }

    pub fn validate(&self) {
        self.tree.validate();
    }
}

impl<'a, T: RangeTreeKey, O: RangeTreeOps<T>> IntoIterator for &'a RangeTreeCustom<T, O> {
    type Item = (&'a T, &'a T);
    type IntoIter = embed_collections::btree::Iter<'a, T, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/*
    /// Legacy AVL Implementation for reference:

    /// return only when segment overlaps with [start, start+size]
    #[inline]
    pub fn find(&self, start: u64, size: u64) -> Option<Arc<RangeSeg<T>>> {
        if self.root.get_count() == 0 {
            return None;
        }
        assert!(size > 0, "range tree find size={} error", size);
        let end = start + size;
        let rs = RangeSeg { start: Cell::new(start), end: Cell::new(end), ..Default::default() };
        let result = self.root.find(&rs, range_tree_segment_cmp);
        result.get_exact()
    }

    /// return only when segment intersect with [start, size], if multiple segment exists, return the
    /// smallest start
    #[inline]
    pub fn find_contained(&self, start: u64, size: u64) -> Option<&RangeSeg<T>> {
        assert!(size > 0, "range tree find size={} error", size);
        if self.root.get_count() == 0 {
            return None;
        }
        let end = start + size;
        let rs_search = RangeSeg { start: Cell::new(start), end: Cell::new(end), ..Default::default() };
        self.root.find_contained(&rs_search, range_tree_segment_cmp)
    }

    #[inline]
    pub fn iter(&self) -> RangeTreeIter<'_, T> {
        RangeTreeIter { tree: self, current: self.root.first() }
    }

    #[inline]
    pub fn walk<F: FnMut(&RangeSeg<T>)>(&self, mut cb: F) {
        let mut node = self.root.first();
        while let Some(_node) = node {
            cb(_node);
            node = self.root.next(_node);
        }
    }

    /// If cb returns false, break
    #[inline]
    pub fn walk_conditioned<F: FnMut(&RangeSeg<T>) -> bool>(&self, mut cb: F) {
        let mut node = self.root.first();
        while let Some(_node) = node {
            if !cb(_node) {
                break;
            }
            node = self.root.next(_node);
        }
    }
*/
