use captains_log::*;
use embed_collections::btree::BTreeMap;
use range_tree_rs::*;
use rstest::rstest;

mod common;
use common::setup_log;

// key is (size, offset)
struct SizeTree(BTreeMap<(u64, u64), ()>);

impl SizeTree {
    fn new() -> Self {
        Self(BTreeMap::new())
    }

    fn verify(&self, rt: &RangeTree<u64>) {
        self.0.validate();
        for (start, size) in rt {
            assert!(self.0.contains_key(&(*size, *start)), "{start}:{size} not in size_tree");
        }
        for (size, start) in self.0.keys() {
            assert_eq!(rt.range(*start..).next(), Some((*start, *size)));
        }
        assert_eq!(rt.len(), self.0.len());
    }
}

impl RangeTreeOps<u64> for SizeTree {
    #[inline(always)]
    fn op_add(&mut self, start: u64, size: u64) {
        self.0.insert((size, start), ());
    }

    #[inline(always)]
    fn op_remove(&mut self, start: u64, size: u64) {
        self.0.remove(&(size, start));
    }
}

//fn range_tree_print<T: RangeTreeKey>(rt: &RangeTree<T>) {
//    for (&k, &v) in rt.iter() {
//        println!("[{}, {}]", k, k + v);
//    }
//}

#[logfn]
#[rstest]
fn range_tree_size(setup_log: ()) {
    let size = core::mem::size_of::<RangeTree<u64>>();
    println!("size {size}");
}

#[logfn]
#[rstest]
fn range_tree_add(setup_log: ()) {
    let mut size_tree = SizeTree::new();
    let mut rt = RangeTree::<u64>::new();
    assert_eq!(0, rt.get_space());

    // 1. Initial add 0:2
    rt.add_with(0, 2, &mut size_tree).unwrap();
    assert_eq!(2, rt.get_space());
    assert_eq!(1, rt.len());
    assert_eq!(rt.collect(), vec![(0, 2)]);
    assert_eq!(rt.add_with(1, 2, &mut size_tree), Err((0, 2)));
    assert_eq!(rt.add_with(1, 3, &mut size_tree), Err((0, 2)));
    size_tree.verify(&rt);

    // 2. Disconnected add 10:5
    rt.add_with(10, 5, &mut size_tree).unwrap();
    assert_eq!(7, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 2), (10, 5)]);
    size_tree.verify(&rt);

    // 3. Right adjacency (merge with left segment 0:2)
    // Add: 2:3
    rt.add_with(2, 3, &mut size_tree).unwrap();
    assert_eq!(10, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 5), (10, 5)]);
    size_tree.verify(&rt);

    // 4. Left adjacency (merge with right segment 10:5)
    // Add: 8:2
    rt.add_with(8, 2, &mut size_tree).unwrap();
    assert_eq!(12, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 5), (8, 7)]);
    size_tree.verify(&rt);
    assert_eq!(rt.add_with(9, 10, &mut size_tree), Err((8, 7)));

    // 5. Double adjacency (merge left 0:5 and right 8:7)
    // Add: 5:3
    rt.add_with(5, 3, &mut size_tree).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.len());
    assert_eq!(rt.collect(), vec![(0, 15)]);
    size_tree.verify(&rt);

    // 6. Overlap errors
    // Existing: 0:15
    // Exact match overlap
    assert_eq!(rt.add(0, 15), Err((0, 15)));
    // Partial overlap start
    assert_eq!(rt.add(0, 5), Err((0, 15)));
    // Partial overlap end
    assert_eq!(rt.add(10, 5), Err((0, 15)));
    // Complete containment
    assert_eq!(rt.add(2, 3), Err((0, 15)));

    size_tree.verify(&rt);

    // Add another disconnected segment for edge bounds tests
    rt.add_with(20, 5, &mut size_tree).unwrap(); // 20:5
    assert_eq!(rt.collect(), vec![(0, 15), (20, 5)]);
    // Straddling left bound (overlaps start of 20:5)
    assert_eq!(rt.add(18, 5), Err((20, 5)));
    // Straddling right bound (overlaps end of 20:5)
    assert_eq!(rt.add(22, 5), Err((20, 5)));
    // Overlapping multiple segments
    assert_eq!(rt.add(14, 8), Err((0, 15)));

    assert_eq!(rt.collect(), vec![(0, 15), (20, 5)]);

    size_tree.verify(&rt);
    rt.validate();
}

#[logfn]
#[rstest]
fn range_tree_add_loosely(setup_log: ()) {
    let mut rt = RangeTree::<u64>::new();
    assert_eq!(0, rt.get_space());

    rt.add_loosely(0, 2);
    assert_eq!(2, rt.get_space());
    assert_eq!(1, rt.len());
    assert_eq!(rt.collect(), vec![(0, 2)]);

    // left join
    rt.add_loosely(2, 5);
    assert_eq!(7, rt.get_space());
    assert_eq!(1, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7)]);

    // without join
    rt.add_loosely(15, 5);
    assert_eq!(12, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7), (15, 5)]);

    // right join
    rt.add_loosely(13, 2);
    assert_eq!(14, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7), (13, 7)]);

    // left intersect
    rt.add_loosely(14, 8);
    assert_eq!(16, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7), (13, 9)]);

    // without join
    rt.add_loosely(25, 5);
    assert_eq!(21, rt.get_space());
    assert_eq!(3, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7), (13, 9), (25, 5)]);

    // duplicate
    rt.add_loosely(12, 20);
    assert_eq!(27, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7), (12, 20)]);

    // left and right intersect
    rt.add_loosely(6, 7);
    assert_eq!(32, rt.get_space());
    assert_eq!(1, rt.len());
    assert_eq!(rt.collect(), vec![(0, 32)]);

    rt.add(50, 10).expect("ok");
    assert_eq!(rt.collect(), vec![(0, 32), (50, 10)]);

    // right intersect
    rt.add_loosely(45, 10);
    assert_eq!(rt.collect(), vec![(0, 32), (45, 15)]);
    rt.add_loosely(70, 5);
    rt.add_loosely(80, 5);
    rt.add_loosely(90, 5);
    rt.add_loosely(100, 5);
    assert_eq!(rt.collect(), vec![(0, 32), (45, 15), (70, 5), (80, 5), (90, 5), (100, 5)]);
    rt.add_loosely(65, 30);
    assert_eq!(rt.collect(), vec![(0, 32), (45, 15), (65, 30), (100, 5)]);

    rt.validate();
}

#[logfn]
#[rstest]
fn range_tree_remove(setup_log: ()) {
    let mut size_tree = SizeTree::new();
    let mut rt = RangeTree::<u64>::new();
    // add [0, 15]
    rt.add_with(0, 15, &mut size_tree).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.len());
    assert_eq!(rt.collect(), vec![(0, 15)]);

    // remove split
    rt.remove_with(7, 1, &mut size_tree).expect("ok");
    assert_eq!(14, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7), (8, 7)]);

    // remove 8-15 shorten
    rt.remove_with(12, 3, &mut size_tree).expect("ok");
    assert_eq!(11, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7), (8, 4)]);

    // remove not existing
    assert_eq!(rt.remove_with(12, 3, &mut size_tree), Err(None));
    assert_eq!(rt.remove_with(15, 3, &mut size_tree), Err(None));

    // remove left intersect error 13 is over 8-12
    assert_eq!(rt.remove_with(10, 3, &mut size_tree), Err(Some((8, 4))));

    // remove [2, 5] expect [0, 2] [5, 7] [8, 12]
    rt.remove_with(2, 3, &mut size_tree).expect("ok");
    assert_eq!(8, rt.get_space());
    assert_eq!(3, rt.len());
    assert_eq!(rt.collect(), vec![(0, 2), (5, 2), (8, 4)]);

    // remove right intersect error, it does not detect the range at the right
    assert_eq!(rt.remove_with(3, 5, &mut size_tree), Err(None));

    // remove [8, 10] expect [0, 2] [5, 7] [10, 12]
    rt.remove_with(8, 2, &mut size_tree).expect("ok");
    assert_eq!(6, rt.get_space());
    assert_eq!(3, rt.len());
    assert_eq!(rt.collect(), vec![(0, 2), (5, 2), (10, 2)]);

    // remove [0, 2] expect [5, 7] [10, 12]
    rt.remove_with(0, 2, &mut size_tree).expect("ok");
    assert_eq!(4, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(5, 2), (10, 2)]);

    size_tree.verify(&rt);

    rt.validate();
}

#[logfn]
#[rstest]
fn range_tree_iter(setup_log: ()) {
    let mut rt = RangeTree::<u64>::new();
    rt.add(0, 2).unwrap();
    rt.add(4, 4).unwrap();
    rt.add(12, 8).unwrap();
    rt.add(32, 16).unwrap();

    let mut count = 0;
    let mut total_space = 0;
    for (&_start, &size) in rt.iter() {
        count += 1;
        total_space += size;
    }
    assert_eq!(count, rt.len() as usize);
    assert_eq!(total_space, rt.get_space());
    assert_eq!(4, count);
    assert_eq!(30, total_space);

    // Test IntoIterator
    let ranges_from_into_iter: Vec<(u64, u64)> =
        (&rt).into_iter().map(|(&start, &size)| (start, size)).collect();
    assert_eq!(ranges_from_into_iter, vec![(0, 2), (4, 4), (12, 8), (32, 16)]);

    // Test for loop on reference
    let mut ranges_from_for: Vec<(u64, u64)> = Vec::new();
    for (&start, &size) in &rt {
        ranges_from_for.push((start, size));
    }
    assert_eq!(ranges_from_for, vec![(0, 2), (4, 4), (12, 8), (32, 16)]);

    // Verify via range iterator
    let rs: Vec<(u64, u64)> = rt.range(..).collect();
    assert_eq!(rs, vec![(0, 2), (4, 4), (12, 8), (32, 16)]);
}

#[logfn]
#[rstest]
fn range_tree_remove_loosely1(setup_log: ()) {
    let mut rt = RangeTree::<u64>::new();

    // add [0, 15]
    rt.add(0, 15).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.len());
    assert_eq!(rt.collect(), vec![(0, 15)]);

    // punch hole
    assert_eq!(rt.remove_loosely(7, 3), true);
    assert_eq!(12, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7), (10, 5)]);

    // remove 13-18 outside 10-15
    assert_eq!(rt.remove_loosely(13, 5), true);
    assert_eq!(10, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7), (10, 3)]);

    // remove nothing [9, 10] expect [0, 7] [10, 13]
    assert_eq!(rt.remove_loosely(9, 1), false);
    assert_eq!(10, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 7), (10, 3)]);

    // remove left over [9, 11] expect [0, 7] [11, 13]
    assert_eq!(rt.remove_loosely(9, 2), true);
    assert_eq!(9, rt.get_space());
    assert_eq!(2, rt.len());
    let rs: Vec<(u64, u64)> = rt.range(..).collect();
    assert_eq!(rs, vec![(0, 7), (11, 2)]);

    // remove [6, 12] expect [0, 6] [12, 13]
    assert_eq!(rt.remove_loosely(6, 6), true);
    assert_eq!(7, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(0, 6), (12, 1)]);
    rt.add(15, 1).expect("ok");
    rt.add(17, 1).expect("ok");
    rt.add(19, 1).expect("ok");
    rt.add(21, 1).expect("ok");
    assert_eq!(rt.collect(), vec![(0, 6), (12, 1), (15, 1), (17, 1), (19, 1), (21, 1)]);
    assert_eq!(rt.remove_loosely(18, 6), true);
    assert_eq!(rt.collect(), vec![(0, 6), (12, 1), (15, 1), (17, 1)]);
    rt.validate();
}

#[logfn]
#[rstest]
fn range_tree_remove_loosely2(setup_log: ()) {
    let mut rt = RangeTree::<u64>::new();

    // add [1, 16]
    rt.add(1, 15).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.len());
    assert_eq!(rt.collect(), vec![(1, 15)]);

    // add [33, 48]
    rt.add(33, 15).unwrap();
    assert_eq!(30, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(1, 15), (33, 15)]);

    // add [49, 64]
    rt.add(49, 15).unwrap();
    assert_eq!(45, rt.get_space());
    assert_eq!(3, rt.len());
    assert_eq!(rt.collect(), vec![(1, 15), (33, 15), (49, 15)]);

    // remove left over and right over [6, 56] expect [1, 6] [56, 64]
    assert_eq!(rt.remove_loosely(6, 50), true);
    assert_eq!(13, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(1, 5), (56, 8)]);
    rt.validate();
}

#[logfn]
#[rstest]
fn range_tree_remove_loosely3(setup_log: ()) {
    let mut rt = RangeTree::<u64>::new();

    // add [1, 16]
    rt.add(1, 15).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.len());
    assert_eq!(rt.collect(), vec![(1, 15)]);

    // add [33, 48]
    rt.add(33, 15).unwrap();
    assert_eq!(30, rt.get_space());
    assert_eq!(2, rt.len());
    assert_eq!(rt.collect(), vec![(1, 15), (33, 15)]);

    // remove left over [0, 40] expect [40, 48]
    assert_eq!(rt.remove_loosely(0, 40), true);
    assert_eq!(8, rt.get_space());
    assert_eq!(1, rt.len());
    assert_eq!(rt.collect(), vec![(40, 8)]);

    rt.validate();
}

#[logfn]
#[rstest]
fn range_tree_find(setup_log: ()) {
    let mut rt = RangeTree::<u64>::new();
    rt.add_abs(2044, 2052).unwrap();
    rt.add_abs(4092, 4096).unwrap();
    rt.add_abs(516096, 516098).unwrap();
    rt.add_abs(518140, 518148).unwrap();
    rt.add_abs(520188, 520194).unwrap();
    rt.add_abs(522236, 522244).unwrap();
    rt.add_abs(524284, 524288).unwrap();
    rt.add_abs(66060288, 66060290).unwrap();
    rt.add_abs(66062332, 66062340).unwrap();
    rt.add_abs(66064380, 66064384).unwrap();
    let (rs_start, rs_size) = rt.range(0..4096).next().unwrap();
    assert_eq!(rs_start, 2044);
    assert_eq!(rs_size, 8);
    for i in &[4096, 516098, 518148, 520194, 522244, 524288, 66060290, 66062340, 66064384] {
        let find = rt.range(4000..*i).next().unwrap();
        assert_eq!(find, (4092, 4));
    }
    assert_eq!(
        rt.range(4093..).collect::<Vec<(u64, u64)>>(),
        vec![
            (4092, 4),
            (516096, 2),
            (518140, 8),
            (520188, 6),
            (522236, 8),
            (524284, 4),
            (66060288, 2),
            (66062332, 8),
            (66064380, 4)
        ]
    );
    assert_eq!(rt.range(4093..518140).collect::<Vec<(u64, u64)>>(), vec![(4092, 4), (516096, 2),]);
    assert_eq!(
        rt.range(4091..=518140).collect::<Vec<(u64, u64)>>(),
        vec![(4092, 4), (516096, 2), (518140, 8),]
    );
    assert_eq!(
        rt.range(..=518140).collect::<Vec<(u64, u64)>>(),
        vec![(2044, 8), (4092, 4), (516096, 2), (518140, 8),]
    );
    assert_eq!(
        rt.range(2044..=518140).collect::<Vec<(u64, u64)>>(),
        vec![(2044, 8), (4092, 4), (516096, 2), (518140, 8),]
    );
    let _space1 = rt.get_space();
    assert!(rt.remove_loosely(0, 66064384));
    assert_eq!(rt.get_space(), 0);
}
