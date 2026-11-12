use kiryl::campaign::choose_next;

fn main() {
    let corpus = include_str!("../../scheduler-cases.tsv");
    assert_eq!(corpus.lines().next(), Some("# scheduler-v1"));
    let mut matched = 0;
    for line in corpus.lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        let charged: Vec<u128> = fields[0]
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect();
        let stopped: Vec<bool> = fields[1]
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| match s {
                "0" => false,
                "1" => true,
                _ => panic!("invalid oracle boolean"),
            })
            .collect();
        let expected: Option<usize> = if fields[2] == "none" {
            None
        } else {
            Some(fields[2].parse().unwrap())
        };
        assert_eq!(
            choose_next(&charged, &stopped),
            expected,
            "oracle mismatch: {line}"
        );
        matched += 1;
    }
    println!("{matched} scheduler cases match the real Rust entry point");
}
