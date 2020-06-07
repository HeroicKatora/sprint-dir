use sprint_dir::WalkDir;

fn main() {
    let dir = std::env::args_os().nth(1).unwrap();
    println!("{}", WalkDir::new(dir).into_iter().count());
}
