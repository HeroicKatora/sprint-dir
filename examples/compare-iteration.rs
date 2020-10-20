use std::time::Instant;
use sprint_dir::WalkDir;

fn main() {
    let start = Instant::now();
    let dir = std::env::args_os().nth(1).unwrap();
    let mut walk = WalkDir::new(&dir).into_iter();
    println!("{}", walk.by_ref().count());
    eprintln!("{:?} {:?}", start.elapsed(), walk.stats());

    let start = Instant::now();
    let mut walk = walkdir::WalkDir::new(dir).into_iter();
    println!("{}", walk.by_ref().count());
    eprintln!("{:?}", start.elapsed());
}
