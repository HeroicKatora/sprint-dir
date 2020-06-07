use sprint_dir::WalkDir;

fn main() {
    let dir = std::env::args_os().nth(1).unwrap();
    let mut walk = WalkDir::new(dir).into_iter();
    println!("{}", walk.by_ref().count());
    eprintln!("{:?}", walk.stats());
}
