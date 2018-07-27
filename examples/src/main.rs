#![feature(use_extern_macros)]
extern crate experiments;
extern crate clap;

use experiments::experiment;

struct MyTest;

#[experiment(flag1: Option<String>: "Dessa forma")]
/// A CLI to test refactoring experiments for crate Thunder
impl MyTest {
    /// Say hello to someone, or the world!
    fn hello(name: Option<String>) {
        println!("Hello, {}!", name.unwrap_or("world".to_string()));
    }
}

fn main() {
    MyTest::start();
}
