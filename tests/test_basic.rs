use range_tree_rs::*;

fn range_tree_print<T: RangeTreeKey, O: RangeTreeOps<T>>(rt: &RangeTreeCustom<T, O>) {
    for (&k, &v) in rt.iter() {
        println!("[{}, {}]", k, k + v);
    }
}

#[test]
fn range_tree_size() {
    let size = core::mem::size_of::<RangeTree<u64>>();
    println!("size {size}");
}

#[test]
fn range_tree_add() {
    let mut rt = RangeTree::<u64>::new();
    assert!(rt.find(0, 10).is_none());
    assert_eq!(0, rt.get_space());

    // 1. Initial add 0:2
    rt.add(0, 2).unwrap();
    assert_eq!(2, rt.get_space());
    assert_eq!(1, rt.get_count());
    let rs = rt.find(0, 1);
    assert!(rs.is_some());
    assert_eq!((0, 2), rs.unwrap());

    // 2. Disconnected add 10:5
    rt.add(10, 5).unwrap();
    assert_eq!(7, rt.get_space());
    assert_eq!(2, rt.get_count());
    assert_eq!(rt.find(11, 1).unwrap(), (10, 5));

    // 3. Right adjacency (merge with left segment 0:2)
    // Add: 2:3
    rt.add(2, 3).unwrap();
    assert_eq!(10, rt.get_space());
    assert_eq!(2, rt.get_count());
    assert_eq!(rt.find(1, 4).unwrap(), (0, 5));

    // 4. Left adjacency (merge with right segment 10:5)
    // Add: 8:2
    rt.add(8, 2).unwrap();
    assert_eq!(12, rt.get_space());
    assert_eq!(2, rt.get_count());
    assert_eq!(rt.find(9, 3).unwrap(), (8, 7));

    // 5. Double adjacency (merge left 0:5 and right 8:7)
    // Add: 5:3
    rt.add(5, 3).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.get_count());
    assert_eq!(rt.find(4, 8).unwrap(), (0, 15));

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

    // Add another disconnected segment for edge bounds tests
    rt.add(20, 5).unwrap(); // 20:5
    // Straddling left bound (overlaps start of 20:5)
    assert_eq!(rt.add(18, 5), Err((20, 5)));
    // Straddling right bound (overlaps end of 20:5)
    assert_eq!(rt.add(22, 5), Err((20, 5)));
    // Overlapping multiple segments
    assert_eq!(rt.add(14, 8), Err((0, 15)));

    rt.validate();
}

#[test]
fn range_tree_add_and_merge() {
    let mut rt = RangeTree::<u64>::new();
    assert!(rt.find(0, 10).is_none());
    assert_eq!(0, rt.get_space());

    rt.add_and_merge(0, 2);
    assert_eq!(2, rt.get_space());
    assert_eq!(1, rt.get_count());

    let rs = rt.find(0, 1);
    assert!(rs.is_some());
    assert_eq!((0, 2), rs.unwrap());

    assert!(rt.find(0, 3).is_some());

    // left join
    rt.add_and_merge(2, 5);
    assert_eq!(7, rt.get_space());
    assert_eq!(1, rt.get_count());

    let rs = rt.find(0, 1);
    assert!(rs.is_some());
    assert_eq!((0, 7), rs.unwrap());

    // without join
    rt.add_and_merge(15, 5);
    assert_eq!(12, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(16, 1);
    assert!(rs.is_some());
    assert_eq!((15, 5), rs.unwrap());

    // right join
    rt.add_and_merge(13, 2);
    assert_eq!(14, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(16, 1);
    assert!(rs.is_some());
    assert_eq!((13, 7), rs.unwrap());

    // duplicate
    rt.add_and_merge(14, 8);
    assert_eq!(16, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(0, 1);
    assert!(rs.is_some());
    assert_eq!((0, 7), rs.unwrap());

    let rs = rt.find(16, 1);
    assert!(rs.is_some());
    assert_eq!((13, 9), rs.unwrap());

    // without join
    rt.add_and_merge(25, 5);
    assert_eq!(21, rt.get_space());
    assert_eq!(3, rt.get_count());

    let rs = rt.find(26, 1);
    assert!(rs.is_some());
    assert_eq!((25, 5), rs.unwrap());

    // duplicate
    rt.add_and_merge(12, 20);
    assert_eq!(27, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(0, 1);
    assert!(rs.is_some());
    assert_eq!((0, 7), rs.unwrap());

    let rs = rt.find(16, 1);
    assert!(rs.is_some());
    assert_eq!((12, 20), rs.unwrap());

    // left and right join
    rt.add_and_merge(7, 5);
    assert_eq!(32, rt.get_space());
    assert_eq!(1, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((0, 32), rs.unwrap());

    rt.validate();
}

#[test]
fn range_tree_remove() {
    let mut rt = RangeTree::<u64>::new();
    // add [0, 15]
    rt.add(0, 15).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.get_count());

    // remove [7, 8] expect [0, 7] [8, 15]
    rt.remove(7, 1);
    assert_eq!(14, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((8, 7), rs.unwrap());
    rt.validate();

    // remove [12, 15] expect [0, 7] [8, 12]
    rt.remove(12, 3);
    assert_eq!(11, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((8, 4), rs.unwrap());
    rt.validate();

    // remove [2, 5] expect [0, 2] [5, 7] [8, 12]
    rt.remove(2, 3);
    assert_eq!(8, rt.get_space());
    assert_eq!(3, rt.get_count());

    let rs = rt.find(5, 1);
    assert!(rs.is_some());
    assert_eq!((5, 2), rs.unwrap());
    rt.validate();

    // remove [8, 10] expect [0, 2] [5, 7] [10, 12]
    rt.remove(8, 2);
    assert_eq!(6, rt.get_space());
    assert_eq!(3, rt.get_count());

    let rs = rt.find(10, 1);
    assert!(rs.is_some());
    assert_eq!((10, 2), rs.unwrap());
    rt.validate();

    // remove [0, 2] expect [5, 7] [10, 12]
    rt.remove(0, 2);
    assert_eq!(4, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(5, 1);
    assert!(rs.is_some());
    assert_eq!((5, 2), rs.unwrap());
    rt.validate();
}

#[test]
fn range_tree_walk() {
    let mut rt = RangeTree::<u64>::new();
    rt.add(0, 2).unwrap();
    rt.add(4, 4).unwrap();
    rt.add(12, 8).unwrap();
    rt.add(32, 16).unwrap();
    assert_eq!(30, rt.get_space());
    assert_eq!(4, rt.get_count());

    rt.walk(|(start, size)| {
        println!("walk callback cb_print range_seg:[{:?}, {:?}]", start, start + size);
    });
}

#[test]
fn range_tree_iter() {
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
    assert_eq!(count, rt.get_count() as usize);
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
}

#[test]
fn range_tree_find_overlap() {
    let mut rt = RangeTree::<u64>::new();
    rt.add_abs(2044, 2052);
    rt.add_abs(4092, 4096);
    rt.add_abs(516096, 516098);
    rt.add_abs(518140, 518148);
    rt.add_abs(520188, 520194);
    rt.add_abs(522236, 522244);
    rt.add_abs(524284, 524288);
    rt.add_abs(66060288, 66060290);
    rt.add_abs(66062332, 66062340);
    rt.add_abs(66064380, 66064384);
    let (rs_start, rs_size) = rt.find(0, 4096).unwrap();
    assert_eq!(rs_start, 2044);
    assert_eq!(rs_size, 8);
    for i in &[4096, 516098, 518148, 520194, 522244, 524288, 66060290, 66062340, 66064384] {
        let (rs_start, _) = rt.find(4000, *i).unwrap();
        assert_eq!(rs_start, 4092);
    }
    range_tree_print(&rt);
    let _space1 = rt.get_space();
    assert!(rt.remove(0, 66064384));
    assert!(rt.get_space() > 0, "only remove one");
    range_tree_print(&rt);
    rt.remove_and_split(0, 66064384); // remove all
    assert_eq!(rt.get_space(), 0);
}

#[test]
fn range_tree_find_overlap_simple() {
    let mut rt = RangeTree::<u64>::new();
    rt.add_abs(20, 80);
    rt.add_abs(120, 180);
    rt.add_abs(220, 280);
    rt.add_abs(320, 380);
    rt.add_abs(420, 480);
    rt.add_abs(520, 580);
    rt.add_abs(620, 680);
    range_tree_print(&rt);
    let (rs_start, rs_size) = rt.find(240, 340).unwrap();
    assert_eq!(rs_start, 220);
    assert_eq!(rs_size, 60);
}

#[test]
fn range_tree_remove1() {
    let mut rt = RangeTree::<u64>::new();

    // add [0, 15]
    rt.add(0, 15).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.get_count());

    // remove [7, 10] expect [0, 7] [10, 15]
    rt.remove_and_split(7, 3);
    assert_eq!(12, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((10, 5), rs.unwrap());
    rt.validate();

    // remove right over [13, 18] expect [0, 7] [10, 13]
    rt.remove_and_split(13, 5);
    assert_eq!(10, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((10, 3), rs.unwrap());
    rt.validate();

    // remove nothing [9, 10] expect [0, 7] [10, 13]
    assert!(!rt.remove_and_split(9, 1));
    assert_eq!(10, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((10, 3), rs.unwrap());
    rt.validate();

    // remove left over [9, 11] expect [0, 7] [11, 13]
    rt.remove_and_split(9, 2);
    assert_eq!(9, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((11, 2), rs.unwrap());
    rt.validate();

    // remove [6, 12] expect [0, 6] [12, 13]
    rt.remove_and_split(6, 6);
    assert_eq!(7, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(0, 5);
    assert!(rs.is_some());
    assert_eq!((0, 6), rs.unwrap());
    rt.validate();
}

#[test]
fn range_tree_remove2() {
    let mut rt = RangeTree::<u64>::new();

    // add [1, 16]
    rt.add(1, 15).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((1, 15), rs.unwrap());
    rt.validate();

    // remove left over and right over [0, 20] expect []
    rt.remove_and_split(0, 20);
    assert_eq!(0, rt.get_space());
    assert_eq!(0, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_none());
    rt.validate();

    // add [1, 16]
    rt.add(1, 15).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((1, 15), rs.unwrap());
    rt.validate();
}

#[test]
fn range_tree_remove3() {
    let mut rt = RangeTree::<u64>::new();

    // add [1, 16]
    rt.add(1, 15).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((1, 15), rs.unwrap());
    rt.validate();

    // add [33, 48]
    rt.add(33, 15).unwrap();
    assert_eq!(30, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(40, 1);
    assert!(rs.is_some());
    assert_eq!((33, 15), rs.unwrap());
    rt.validate();

    // add [49, 64]
    rt.add(49, 15).unwrap();
    assert_eq!(45, rt.get_space());
    assert_eq!(3, rt.get_count());

    let rs = rt.find(50, 1);
    assert!(rs.is_some());
    assert_eq!((49, 15), rs.unwrap());
    rt.validate();

    // remove left over and right over [6, 56] expect [1, 6] [56, 64]
    rt.remove_and_split(6, 50);
    assert_eq!(13, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(58, 1);
    assert!(rs.is_some());
    assert_eq!((56, 8), rs.unwrap());
    rt.validate();

    let rs = rt.find(3, 1);
    assert!(rs.is_some());
    assert_eq!((1, 5), rs.unwrap());
    rt.validate();
}

#[test]
fn range_tree_remove4() {
    let mut rt = RangeTree::<u64>::new();

    // add [1, 16]
    rt.add(1, 15).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((1, 15), rs.unwrap());
    rt.validate();

    // add [33, 48]
    rt.add(33, 15).unwrap();
    assert_eq!(30, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(40, 1);
    assert!(rs.is_some());
    assert_eq!((33, 15), rs.unwrap());
    rt.validate();

    // remove right over [6, 56] expect [1, 6]
    rt.remove_and_split(6, 50);
    assert_eq!(5, rt.get_space());
    assert_eq!(1, rt.get_count());

    let rs = rt.find(3, 1);
    assert!(rs.is_some());
    assert_eq!((1, 5), rs.unwrap());
    rt.validate();
}

#[test]
fn range_tree_remove5() {
    let mut rt = RangeTree::<u64>::new();

    // add [1, 16]
    rt.add(1, 15).unwrap();
    assert_eq!(15, rt.get_space());
    assert_eq!(1, rt.get_count());

    let rs = rt.find(11, 1);
    assert!(rs.is_some());
    assert_eq!((1, 15), rs.unwrap());
    rt.validate();

    // add [33, 48]
    rt.add(33, 15).unwrap();
    assert_eq!(30, rt.get_space());
    assert_eq!(2, rt.get_count());

    let rs = rt.find(40, 1);
    assert!(rs.is_some());
    assert_eq!((33, 15), rs.unwrap());
    rt.validate();

    // remove left over [0, 40] expect [40, 48]
    rt.remove_and_split(0, 40);
    assert_eq!(8, rt.get_space());
    assert_eq!(1, rt.get_count());

    let rs = rt.find(42, 1);
    assert!(rs.is_some());
    assert_eq!((40, 8), rs.unwrap());
    rt.validate();
}
