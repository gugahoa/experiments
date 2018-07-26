#![feature(use_extern_macros)]
extern crate experiments;
extern crate clap;

use experiments::experiment;

struct MyTest;

#[experiment(flag1: Option<String>: "Dessa forma")]
/// A CLI to test refactoring experiments for crate Thunder
impl MyTest {

}

fn main() {
    MyTest::start();
}
