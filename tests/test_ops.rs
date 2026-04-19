use embed_collections::btree::BTreeMap;
use range_tree_rs::*;

// Test RangeTreeOps using a Size Tree pattern
pub struct TestAllocator {
    // Key is (size, start), Value must be non-ZST for embed-collections BTreeMap
    pub size_tree: BTreeMap<(u64, u64), u8>,
}

impl Default for TestAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl TestAllocator {
    pub fn new() -> Self {
        TestAllocator { size_tree: BTreeMap::new() }
    }

    /// Simulates slab allocation's Best-fit selection
    pub fn find_best_fit(&self, min_size: u64) -> Option<(u64, u64)> {
        // Search for the smallest size that is >= min_size
        self.size_tree.range((min_size, 0)..).next().map(|(&key, _)| key)
    }
}

impl RangeTreeOps<u64> for TestAllocator {
    fn op_add(&mut self, start: u64, end: u64) {
        let size = end - start;
        self.size_tree.insert((size, start), 0);
    }

    fn op_remove(&mut self, start: u64, end: u64) {
        let size = end - start;
        self.size_tree.remove(&(size, start));
    }
}

#[test]
fn range_tree_ops_size_tree_linkage() {
    let mut ms_tree = RangeTreeCustom::<u64, TestAllocator>::new();

    assert!(ms_tree.find(0, 10).is_none());
    assert_eq!(0, ms_tree.get_space());

    // 1. Add discrete segments
    ms_tree.add(100, 50).unwrap(); // Segment A: [100, 150], size 50
    ms_tree.add(200, 30).unwrap(); // Segment B: [200, 230], size 30
    ms_tree.add(300, 100).unwrap(); // Segment C: [300, 400], size 100

    assert_eq!(180, ms_tree.get_space());
    assert_eq!(3, ms_tree.get_count());

    // 2. Mock Best-fit allocation logic through size_tree
    let alloc = ms_tree.get_ops();

    // Request size 25: Should pick Segment B (size 30)
    let best_fit = alloc.find_best_fit(25).unwrap();
    assert_eq!(best_fit, (30, 200));

    // Request size 40: Should pick Segment A (size 50)
    let best_fit = alloc.find_best_fit(40).unwrap();
    assert_eq!(best_fit, (50, 100));

    // Request size 60: Should pick Segment C (size 100)
    let best_fit = alloc.find_best_fit(60).unwrap();
    assert_eq!(best_fit, (100, 300));

    // 3. Verify removal synchronization
    ms_tree.remove(100, 50); // Remove Segment A
    assert_eq!(130, ms_tree.get_space());
    assert_eq!(2, ms_tree.get_count());

    let alloc = ms_tree.get_ops();
    let best_fit = alloc.find_best_fit(40).unwrap();
    assert_eq!(best_fit, (100, 300)); // Segment A is gone, next best is Segment C
}

#[test]
fn range_tree_ops_merge_swallow_sync() {
    let mut ms_tree = RangeTreeCustom::<u64, TestAllocator>::new();

    // Add three small segments
    ms_tree.add(10, 10).unwrap(); // [10, 20]
    ms_tree.add(30, 10).unwrap(); // [30, 40]
    ms_tree.add(50, 10).unwrap(); // [50, 60]

    // Confirm size_tree has 3 entries
    assert_eq!(3, ms_tree.get_ops().size_tree.len());

    // Execute add_and_merge to swallow all
    ms_tree.add_and_merge(10, 50); // Resulting segment: [10, 60], size 50

    assert_eq!(1, ms_tree.get_count());
    assert_eq!(50, ms_tree.get_space());

    // Confirm size_tree is synced (should only have one large segment)
    let alloc = ms_tree.get_ops();
    assert_eq!(1, alloc.size_tree.len());
    let best_fit = alloc.find_best_fit(50).unwrap();
    assert_eq!(best_fit, (50, 10)); // Size 50, Start 10
}
