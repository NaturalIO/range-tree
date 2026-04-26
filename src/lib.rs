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

pub use embed_collections::btree::{IntoIter, Iter};

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
    // the tree stores ranges in [key:start, value:size) format
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

    /// Add range segment, merge with adjacent ranges, assuming no intersections.
    ///
    /// Returns `Ok(())` if there are no intersection;
    /// otherwise returns the overlapping range as `Err((existing_start, existing_size))`.
    ///
    /// This equals to add + add_find_overlap in v0.1
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

    /// Add range which may have multiple intersections with existing range, ensuring union result
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
                            // TODO is it possible to remove and get next entry?
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

    /// Remove all the intersection ranges in the tree (might span across multiple range)
    ///
    /// Equals to remove_and_split in v0.1
    ///
    /// return true if overlapping range found and removed
    #[inline]
    pub fn remove(&mut self, mut start: T, mut size: T) -> bool {
        let end = start + size;
        let mut ent = self.tree.entry(start);
        let mut removed = false;
        loop {
            match ent {
                Entry::Occupied(mut oent) => {
                    let rs_size = *oent.get();
                    self.ops.op_remove(start, start + rs_size);
                    if rs_size == size {
                        // Exact match or subset removed
                        oent.remove();
                        self.space -= rs_size;
                        return true;
                    } else if rs_size > size {
                        // Shrink from front
                        let new_start = start + size;
                        let new_size = rs_size - size;
                        oent.alter_key(new_start).expect("shrink alter_key");
                        *oent.get_mut() = new_size;
                        self.ops.op_add(new_start, new_start + new_size);
                        self.space -= size;
                        return true;
                    } else {
                        if let Some((_next_start, _next_size)) = oent.peak_forward() {
                            if *_next_start < end {
                                start = *_next_start;
                                size = end - start;
                                self.space -= *oent.get();
                                oent.remove();
                                ent = self.tree.entry(start);
                                removed = true;
                                continue;
                            }
                        }
                        self.space -= rs_size;
                        oent.remove();
                        return true;
                    }
                }
                Entry::Vacant(vent) => {
                    if let Some((&rs_start, &rs_size)) = vent.peak_backward() {
                        let rs_end = rs_start + rs_size;
                        if rs_end > start {
                            let mut oent = vent.move_backward().expect("move back to overlapping");
                            self.ops.op_remove(rs_start, rs_end);
                            if rs_end > end {
                                // punch a hold in the middle
                                *oent.get_mut() = start - rs_start;
                                // TODO optimize add insert after entry for btree
                                self.tree.insert(end, rs_end - end);
                                self.ops.op_add(rs_start, start);
                                self.ops.op_add(end, rs_end);
                                self.space -= size;
                                return true;
                            } else {
                                // Shrink from back
                                *oent.get_mut() = start - rs_start;
                                self.ops.op_add(rs_start, start);
                                self.space -= rs_end - start;
                                if rs_end == end {
                                    return true;
                                }
                                if let Some((next_start, _)) = oent.peak_forward() {
                                    if *next_start < end {
                                        start = *next_start;
                                        size = end - *next_start;
                                        ent = Entry::Occupied(
                                            oent.move_forward()
                                                .expect("move forward to overlapping"),
                                        );
                                        continue;
                                    }
                                }
                                return true;
                            }
                        }
                    }
                    // Handle the case where range starts before the first overlapping segment
                    if let Some((next_start, _)) = vent.peak_forward() {
                        if *next_start < end {
                            start = *next_start;
                            size = end - *next_start;
                            ent = Entry::Occupied(
                                vent.move_forward().expect("move forward to overlapping"),
                            );
                            continue;
                        }
                    }
                    return removed;
                }
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

    pub fn iter(&self) -> Iter<'_, T, T> {
        self.tree.iter()
    }

    pub fn validate(&self) {
        self.tree.validate();
    }
}

impl<'a, T: RangeTreeKey, O: RangeTreeOps<T>> IntoIterator for &'a RangeTreeCustom<T, O> {
    type Item = (&'a T, &'a T);
    type IntoIter = Iter<'a, T, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: RangeTreeKey, O: RangeTreeOps<T>> IntoIterator for RangeTreeCustom<T, O> {
    type Item = (T, T);
    type IntoIter = IntoIter<T, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.tree.into_iter()
    }
}
