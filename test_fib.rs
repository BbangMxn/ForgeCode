fn fib(n: u64) -> u64 {
    let (a, b) = (0, 1);
    let mut x = a;
    let mut y = b;
    for _ in 2..n {
        let next = x + y;
        x = y;
        y = next;
    }
    y
}

fn main() {
    for i in 0..11 {
        println!("fib({i}) = {}", fib(i));
    }
}