#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]

//! This crate provides a range-tree implementation, intended to manage range section with btree.

use core::{
    cmp::Ordering,
    fmt,
    ops::{AddAssign, Bound, RangeBounds, SubAssign},
};
use embed_collections::btree::{BTreeMap, Entry};
use num_traits::*;

pub use embed_collections::btree::{Cursor, IntoIter, Iter};

pub trait RangeTreeKey:
    Unsigned + AddAssign + SubAssign + Ord + Copy + fmt::Debug + fmt::Display + Default + 'static
{
}

impl<T> RangeTreeKey for T where
    T: Unsigned
        + AddAssign
        + SubAssign
        + Ord
        + Copy
        + fmt::Debug
        + fmt::Display
        + Default
        + 'static
{
}

pub struct RangeTree<T: RangeTreeKey> {
    // the tree stores ranges in [key:start, value:size) format
    tree: BTreeMap<T, T>,
    space: T,
}

/// Trait for allocator
///
/// when range tree merge/split, need to mirror the adding and removal range from size_tree
pub trait RangeTreeOps<T: RangeTreeKey> {
    /// Callback for manage size tree
    fn op_add(&mut self, start: T, size: T);
    /// Callback for manage size tree
    fn op_remove(&mut self, start: T, size: T);
}

#[derive(Default)]
pub struct DummyOps();

impl<T: RangeTreeKey> RangeTreeOps<T> for DummyOps {
    #[inline]
    fn op_add(&mut self, _start: T, _size: T) {}

    #[inline]
    fn op_remove(&mut self, _start: T, _size: T) {}
}

impl<T: RangeTreeKey> RangeTree<T> {
    pub fn new() -> Self {
        Self { tree: BTreeMap::new(), space: T::zero() }
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
    pub fn len(&self) -> usize {
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
        self.add_with(start, size, &mut DummyOps {})
    }

    #[inline]
    pub fn add_with<O>(&mut self, start: T, size: T, ops: &mut O) -> Result<(), (T, T)>
    where
        O: RangeTreeOps<T>,
    {
        assert!(size > T::zero(), "range tree add size={} error", size);
        let end = start + size;
        let mut prev = None;
        let mut next = None;
        match self.tree.entry(start) {
            Entry::Occupied(ent) => {
                return Err((*ent.key(), *ent.get()));
            }
            Entry::Vacant(ent) => {
                if let Some((_start, _size)) = ent.peek_backward() {
                    let _end = *_start + *_size;
                    match _end.cmp(&start) {
                        Ordering::Equal => {
                            prev = Some((*_start, *_size));
                        }
                        Ordering::Greater => return Err((*_start, *_size)),
                        _ => {}
                    }
                }
                if let Some((_start, _size)) = ent.peek_forward() {
                    match end.cmp(_start) {
                        Ordering::Equal => {
                            next = Some((*_start, *_size));
                        }
                        Ordering::Greater => return Err((*_start, *_size)),
                        _ => {}
                    }
                }
                match (prev, next) {
                    (None, None) => {
                        ops.op_add(start, size);
                        ent.insert(size);
                    }
                    (None, Some((next_start, mut next_size))) => {
                        let mut ent_next = ent.move_forward().expect("merge next");
                        ops.op_remove(next_start, next_size);
                        next_size += size;
                        *ent_next.get_mut() = next_size;
                        ent_next.alter_key(start).expect("merge next alter_key");
                        ops.op_add(start, next_size);
                    }
                    (Some((prev_start, mut prev_size)), None) => {
                        ops.op_remove(prev_start, prev_size);
                        let mut ent_prev = ent.move_backward().expect("merge prev");
                        prev_size += size;
                        *ent_prev.get_mut() = prev_size;
                        ops.op_add(prev_start, prev_size);
                    }
                    (Some((prev_start, prev_size)), Some((next_start, next_size))) => {
                        ops.op_remove(prev_start, prev_size);
                        ops.op_remove(next_start, next_size);
                        let mut ent_prev = ent.move_backward().expect("merge prev");
                        let final_size = prev_size + size + next_size;
                        *ent_prev.get_mut() = final_size;
                        ops.op_add(prev_start, final_size);
                        let ent_next = ent_prev.move_forward().expect("merge next");
                        ent_next.remove();
                    }
                }
                self.space += size;
                Ok(())
            }
        }
    }

    #[inline(always)]
    pub fn add_abs(&mut self, start: T, end: T) -> Result<(), (T, T)> {
        assert!(start < end, "range tree add start={} end={}", start, end);
        self.add(start, end - start)
    }

    /// Add range which may have multiple intersections with existing range, ensuring union result
    #[inline]
    pub fn add_loosely(&mut self, start: T, size: T) {
        assert!(size > T::zero(), "range tree add size error");
        let new_end = start + size;
        let base_ent = match self.tree.entry(start) {
            Entry::Occupied(oe) => {
                if start + *oe.get() >= new_end {
                    return;
                }
                Entry::Occupied(oe)
            }
            Entry::Vacant(ve) => {
                if let Some((pre_start, pre_size)) = ve.peek_backward() {
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
                    |_removed_start, removed_size| {
                        self.space -= *removed_size;
                    },
                ) {
                    let last_end = last_start + last_size;
                    if last_end > new_end {
                        let _size = last_end - new_end;
                        // add back and join with previous range
                        self.add(new_end, _size)
                            .expect("add {new_end:?}:{_size:?} should not fail");
                    }
                }
            };
        }
        match base_ent {
            Entry::Occupied(mut oe) => {
                let base_start = *oe.key();
                let old_size = *oe.get();

                // extend the size to final size
                let final_size = new_end - base_start;
                self.space += final_size - old_size;
                *oe.get_mut() = final_size;

                if let Some((_next_start, _next_size)) = oe.peek_forward() {
                    let next_start = *_next_start;
                    let next_size = *_next_size;
                    if next_start < new_end {
                        drop(oe);
                        remove_intersect!(next_start, new_end);
                    } else if next_start == new_end {
                        // space is neutral (moving between segments)
                        *oe.get_mut() += next_size;
                        self.tree.remove(&next_start);
                    }
                }
            }
            Entry::Vacant(ve) => {
                let base_start = start;
                self.space += size;

                if let Some((_next_start, _next_size)) = ve.peek_forward() {
                    let next_start = *_next_start;
                    let next_size = *_next_size;
                    if next_start < new_end {
                        ve.insert(size);
                        remove_intersect!(next_start, new_end);
                    } else if next_start == new_end {
                        let final_size = new_end - base_start + next_size;
                        ve.insert(final_size);
                        self.tree.remove(&next_start);
                    } else {
                        ve.insert(size);
                    }
                } else {
                    ve.insert(size);
                }
            }
        }
    }

    /// Valid and remove specify range start:size
    ///
    /// # Return value
    /// - Only return Ok(()) when there's existing range equal to or contain the removal range in the tree,
    /// - return Err(None) when not found,
    /// - return Err(Some(start, size)) when a range intersect with the removal range, or when the
    ///   removal range larger than existing range.
    #[inline]
    pub fn remove(&mut self, start: T, size: T) -> Result<(), Option<(T, T)>> {
        self.remove_with(start, size, &mut DummyOps {})
    }

    /// Valid and remove specify range start:size
    ///
    /// # Return value
    ///
    /// - Only return Ok(()) when there's existing range equal to or contain the removal range in the tree,
    /// - return Err(None) when not found,
    /// - return Err(Some(start, size)) when a range intersect with the removal range, or when the
    ///   removal range larger than existing range.
    pub fn remove_with<O>(&mut self, start: T, size: T, ops: &mut O) -> Result<(), Option<(T, T)>>
    where
        O: RangeTreeOps<T>,
    {
        let end = start + size;
        let ent = self.tree.entry(start);
        match ent {
            Entry::Occupied(mut oent) => {
                let rs_size = *oent.get();
                ops.op_remove(start, rs_size);
                if rs_size == size {
                    // Exact match or subset removed
                    oent.remove();
                    self.space -= rs_size;
                    return Ok(());
                } else if rs_size > size {
                    // Shrink from front
                    let new_start = start + size;
                    let new_size = rs_size - size;
                    oent.alter_key(new_start).expect("shrink alter_key");
                    *oent.get_mut() = new_size;
                    ops.op_add(new_start, new_size);
                    self.space -= size;
                    return Ok(());
                } else {
                    // existing range smaller than what need to remove
                    return Err(Some((start, rs_size)));
                }
            }
            Entry::Vacant(vent) => {
                if let Some((&rs_start, &rs_size)) = vent.peek_backward() {
                    let rs_end = rs_start + rs_size;
                    if rs_end > start {
                        ops.op_remove(rs_start, rs_size);
                        let mut oent = vent.move_backward().expect("move back to overlapping");
                        if rs_end > end {
                            let new_size = start - rs_start;
                            // punch a hold in the middle
                            *oent.get_mut() = new_size;
                            ops.op_add(rs_start, new_size);
                            let new_size2 = rs_end - end;
                            // TODO optimize add insert after entry for btree
                            self.tree.insert(end, new_size2);
                            ops.op_add(end, new_size2);
                            self.space -= size;
                            return Ok(());
                        } else if rs_end == end {
                            // Shrink from back
                            let new_size = start - rs_start;
                            *oent.get_mut() = new_size;
                            ops.op_add(rs_start, new_size);
                            self.space -= rs_end - start;
                            return Ok(());
                        } else {
                            return Err(Some((rs_start, rs_size)));
                        }
                    } else {
                        return Err(None);
                    }
                } else {
                    return Err(None);
                }
            }
        }
    }

    /// Remove all the intersection ranges in the tree
    ///
    /// the range start:size to remove allow to be larger than the existing range
    ///
    /// Equals to remove_and_split in v0.1
    ///
    /// return true if overlapping range found and removed.
    /// return false if overlapping range not found.
    ///
    /// #[inline]
    pub fn remove_loosely(&mut self, mut start: T, mut size: T) -> bool {
        let end = start + size;
        let mut ent = self.tree.entry(start);
        let mut removed = false;
        loop {
            match ent {
                Entry::Occupied(mut oent) => {
                    let rs_size = *oent.get();
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
                        self.space -= size;
                        return true;
                    } else {
                        if let Some((_next_start, _next_size)) = oent.peek_forward() {
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
                    if let Some((&rs_start, &rs_size)) = vent.peek_backward() {
                        let rs_end = rs_start + rs_size;
                        if rs_end > start {
                            let mut oent = vent.move_backward().expect("move back to overlapping");
                            if rs_end > end {
                                let new_size = start - rs_start;
                                // punch a hold in the middle
                                *oent.get_mut() = new_size;
                                let new_size2 = rs_end - end;
                                // TODO optimize add insert after entry for btree
                                self.tree.insert(end, new_size2);
                                self.space -= size;
                                return true;
                            } else {
                                // Shrink from back
                                let new_size = start - rs_start;
                                *oent.get_mut() = new_size;
                                self.space -= rs_end - start;
                                if rs_end == end {
                                    return true;
                                }
                                if let Some((next_start, _)) = oent.peek_forward() {
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
                    if let Some((next_start, _)) = vent.peek_forward() {
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
    pub fn range<'a, R: RangeBounds<T>>(&'a self, r: R) -> RangeIter<'a, T> {
        let start = match r.start_bound() {
            Bound::Included(start) => Some(*start),
            Bound::Excluded(start) => Some(*start),
            _ => None,
        };
        let cursor = if let Some(_start) = start {
            let mut _cursor = self.tree.cursor(&_start);
            if let Some((pre_start, pre_size)) = _cursor.peek_backward() {
                let pre_end = *pre_start + *pre_size;
                if pre_end > _start {
                    _cursor.previous();
                }
                // TODO what if we find pre_start < start but pre_start + size >= start
            }
            _cursor
        } else {
            self.tree.first_cursor()
        };
        RangeIter { cursor, end: r.end_bound().cloned(), not_empty: true }
    }

    pub fn collect(&self) -> Vec<(T, T)> {
        let mut v = Vec::with_capacity(self.len());
        for (start, size) in &self.tree {
            v.push((*start, *size))
        }
        v
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, T, T> {
        self.tree.iter()
    }

    pub fn validate(&self) {
        self.tree.validate();
    }

    #[inline]
    pub fn memory_used(&self) -> usize {
        self.tree.memory_used()
    }
}

impl<'a, T: RangeTreeKey> IntoIterator for &'a RangeTree<T> {
    type Item = (&'a T, &'a T);
    type IntoIter = Iter<'a, T, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: RangeTreeKey> IntoIterator for RangeTree<T> {
    type Item = (T, T);
    type IntoIter = IntoIter<T, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.tree.into_iter()
    }
}

pub struct RangeIter<'a, T: RangeTreeKey> {
    cursor: Cursor<'a, T, T>,
    end: Bound<T>,
    not_empty: bool,
}

impl<'a, T: RangeTreeKey> Iterator for RangeIter<'a, T> {
    type Item = (T, T);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.not_empty {
            if let Some((start, size)) = self.cursor.next() {
                match self.end {
                    Bound::Unbounded => return Some((*start, *size)),
                    Bound::Excluded(end) => {
                        if *start < end {
                            return Some((*start, *size));
                        }
                        self.not_empty = false;
                        return None;
                    }
                    Bound::Included(end) => {
                        if *start <= end {
                            return Some((*start, *size));
                        }
                        self.not_empty = false;
                        return None;
                    }
                }
            }
            self.not_empty = false;
        }
        return None;
    }
}
